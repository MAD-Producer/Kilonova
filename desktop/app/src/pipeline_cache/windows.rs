//! Private cache leaf creation/validation using the account's native ACLs.
//! The configured account directory and its ancestors remain trusted input.
use super::{refused, reject_link};
use std::{fs, io, os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree, HANDLE},
    Security::{
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        EqualSid, GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetTokenInformation,
        IsWellKnownSid, TokenUser, WinBuiltinAdministratorsSid, WinLocalSystemSid, ACL,
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        SECURITY_ATTRIBUTES, SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::CreateDirectoryW,
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

// No inherited grants to unrelated accounts. Owner, system and administrators
// can access both the directory and its subsequently created cache snapshots.
const PRIVATE_DACL: &str = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

struct Descriptor(PSECURITY_DESCRIPTOR);
impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: both allocating APIs below document LocalFree ownership.
        if !self.0.is_null() && !unsafe { LocalFree(self.0) }.is_null() {
            log::error!(
                "driver cache descriptor cleanup failed cause={}",
                io::Error::last_os_error()
            );
        }
    }
}
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: this is an owned token handle returned by OpenProcessToken.
        if unsafe { CloseHandle(self.0) } == 0 {
            log::error!(
                "driver cache token cleanup failed cause={}",
                io::Error::last_os_error()
            );
        }
    }
}

fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut name: Vec<_> = path.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return Err(refused("pipeline cache path contains NUL"));
    }
    name.push(0);
    Ok(name)
}

fn private_descriptor() -> io::Result<Descriptor> {
    let text: Vec<_> = PRIVATE_DACL.encode_utf16().chain(Some(0)).collect();
    let mut result = ptr::null_mut();
    // SAFETY: terminated input and valid out pointer; result owns native memory.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut result,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(Descriptor(result))
}

fn descriptor_acl(descriptor: &Descriptor) -> io::Result<*mut ACL> {
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl = ptr::null_mut();
    // SAFETY: descriptor was returned by a successful system descriptor API.
    if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    if present == 0 || acl.is_null() {
        return Err(refused("pipeline cache has no restrictive DACL"));
    }
    Ok(acl)
}

fn trusted_owner(owner: PSID) -> io::Result<bool> {
    if owner.is_null() {
        return Ok(false);
    }
    // SAFETY: owner points into the live system-returned descriptor.
    if unsafe {
        IsWellKnownSid(owner, WinLocalSystemSid) != 0
            || IsWellKnownSid(owner, WinBuiltinAdministratorsSid) != 0
    } {
        return Ok(true);
    }
    let mut handle = ptr::null_mut();
    // SAFETY: pseudo process handle is borrowed, token result is owned below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Token(handle);
    let mut length = 0;
    // SAFETY: the documented size query uses a null zero-length output buffer.
    let queried =
        unsafe { GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut length) };
    let error = io::Error::last_os_error();
    if queried == 0 && error.raw_os_error() != Some(122) {
        return Err(error);
    }
    if length < std::mem::size_of::<TOKEN_USER>() as u32 || length > 65_536 {
        return Err(refused("pipeline cache token user size is invalid"));
    }
    // usize storage provides sufficient alignment for TOKEN_USER and SID data.
    let mut buffer = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
    let capacity = buffer.len() * std::mem::size_of::<usize>();
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            capacity as u32,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if length as usize > capacity {
        return Err(refused("pipeline cache token user exceeded buffer"));
    }
    // SAFETY: successful API result contains TOKEN_USER, owned by this buffer.
    let current = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    Ok(unsafe { EqualSid(owner, current.User.Sid) } != 0)
}

pub(super) fn private_directory(path: &Path) -> io::Result<()> {
    let name = wide(path)?;
    let expected = private_descriptor()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: expected.0,
        bInheritHandle: 0,
    };
    // SAFETY: name and security descriptor stay alive throughout the call.
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } == 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error);
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    reject_link(&metadata)?;
    if !metadata.is_dir() {
        return Err(refused("pipeline cache leaf is not a directory"));
    }
    let mut descriptor = ptr::null_mut();
    let mut owner = ptr::null_mut();
    // SAFETY: all optional outputs may be null. Descriptor owns the owner SID.
    let code = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let actual = Descriptor(descriptor);
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(actual.0, &mut control, &mut revision) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if control & SE_DACL_PROTECTED == 0 || !trusted_owner(owner)? {
        return Err(refused(
            "pipeline cache must have a protected account-owned DACL",
        ));
    }
    let actual_acl = descriptor_acl(&actual)?;
    let expected_acl = descriptor_acl(&expected)?;
    // SAFETY: ACLs belong to validated, live system-returned descriptors. AclSize
    // is the API-defined byte extent (u16), bounded independently of file data.
    let (actual_bytes, expected_bytes) = unsafe {
        (
            std::slice::from_raw_parts(actual_acl.cast::<u8>(), (*actual_acl).AclSize as usize),
            std::slice::from_raw_parts(expected_acl.cast::<u8>(), (*expected_acl).AclSize as usize),
        )
    };
    if actual_bytes != expected_bytes {
        return Err(refused("pipeline cache DACL grants unexpected access"));
    }
    Ok(())
}
