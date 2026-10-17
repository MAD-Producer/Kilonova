//! Published SDK packages used select(a < b, c > d, condition), which the
//! current WGSL template-list discovery rules interpret ambiguously. Group
//! call arguments without changing their expressions or the saved package bytes.
use std::borrow::Cow;

fn comment_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start..start + 2) == Some(b"//") {
        return Some(
            bytes[start..]
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |i| start + i),
        );
    }
    if bytes.get(start..start + 2) != Some(b"/*") {
        return None;
    }
    let (mut i, mut depth) = (start + 2, 1usize);
    while i < bytes.len() && depth != 0 {
        if bytes.get(i..i + 2) == Some(b"/*") {
            depth += 1;
            i += 2;
        } else if bytes.get(i..i + 2) == Some(b"*/") {
            depth -= 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    Some(i)
}

pub(super) fn normalize(source: &str) -> Cow<'_, str> {
    let bytes = source.as_bytes();
    let mut stack: Vec<Option<usize>> = Vec::new();
    let mut edits = Vec::new();
    let (mut i, mut previous, mut select_call) = (0, "", false);
    while i < bytes.len() {
        if let Some(end) = comment_end(bytes, i) {
            i = end;
            continue;
        }
        let byte = bytes[i];
        if byte.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' || byte >= 128 {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] >= 128)
            {
                i += 1;
            }
            let word = &source[start..i];
            select_call = word == "select" && previous != "fn";
            previous = word;
            continue;
        }
        match byte {
            b'(' => stack.push(select_call.then_some(i + 1)),
            b',' => {
                if let Some(Some(start)) = stack.last_mut() {
                    edits.push((*start, '('));
                    edits.push((i, ')'));
                    *start = i + 1;
                }
            }
            b')' => {
                if let Some(Some(start)) = stack.pop() {
                    edits.push((start, '('));
                    edits.push((i, ')'));
                }
            }
            _ => {}
        }
        select_call = false;
        previous = "";
        i += 1;
    }
    if edits.is_empty() {
        return Cow::Borrowed(source);
    }
    // Stable sorting preserves closing-before-opening at adjacent arguments.
    edits.sort_by_key(|edit| edit.0);
    let mut output = String::with_capacity(source.len() + edits.len());
    let mut copied = 0;
    for (position, character) in edits {
        output.push_str(&source[copied..position]);
        output.push(character);
        copied = position;
    }
    output.push_str(&source[copied..]);
    Cow::Owned(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_nested_comparisons_without_changing_comments_or_function_declarations() {
        let input = "// select(bad, text)\n/* outer /* select( */ ) */\nfn select(a: f32, b: f32, c: bool) {}\nselect(v.a < outer.a, select(x > y, z, true), amount < 0.0)";
        assert_eq!(normalize(input), "// select(bad, text)\n/* outer /* select( */ ) */\nfn select(a: f32, b: f32, c: bool) {}\nselect((v.a < outer.a),( select((x > y),( z),( true))),( amount < 0.0))");
        assert!(matches!(
            normalize("fn hello() { return vec2<f32>(1., 2.); }"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn rejects_invalid_expressions_after_normalization() {
        assert!(naga::front::wgsl::parse_str(&normalize(
            "fn f() { let x = select(??, 1., true); }"
        ))
        .is_err());
    }
}
