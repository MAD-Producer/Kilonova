use crate::{invalid, Result};

/// Limit logical lines as well as physical ones, including continued records.
pub(super) fn visit(
    source: &[u8],
    phase: &'static str,
    mut record: impl FnMut(usize, &[String]) -> Result<()>,
) -> Result<()> {
    let text =
        std::str::from_utf8(source).map_err(|_| invalid(phase, "source", "text must be UTF-8"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut joined = String::new();
    let mut first = 1;
    for (n, line) in text.lines().enumerate() {
        if joined.is_empty() {
            first = n + 1;
        }
        if line.len() > 65536 || joined.len().saturating_add(line.len()) > 65536 {
            return Err(invalid(phase, first, "logical line byte limit exceeded"));
        }
        let line = line.trim_end();
        if let Some(part) = line.strip_suffix('\\') {
            joined.push_str(part);
            joined.push(' ');
            continue;
        }
        joined.push_str(line);
        let tokens = words(&joined, phase, first)?;
        if !tokens.is_empty() {
            record(first, &tokens)?;
        }
        joined.clear();
    }
    if !joined.is_empty() {
        return Err(invalid(phase, first, "unterminated line continuation"));
    }
    Ok(())
}

fn words(line: &str, phase: &'static str, number: usize) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => break,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c if c.is_control() => {
                return Err(invalid(phase, number, "control character in record"))
            }
            c => current.push(c),
        }
        if out.len() > 1024 {
            return Err(invalid(phase, number, "token count limit exceeded"));
        }
    }
    if quoted {
        return Err(invalid(phase, number, "unterminated quoted token"));
    }
    if !current.is_empty() {
        out.push(current);
    }
    Ok(out)
}

pub(super) fn number(value: &str, phase: &'static str, line: usize) -> Result<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid(phase, line, "expected a finite numeric component"))
}
pub(super) fn name(values: &[String], phase: &'static str, line: usize) -> Result<String> {
    let value = values.join(" ");
    if value.is_empty() || value.len() > 1024 {
        return Err(invalid(phase, line, "name must contain 1 to 1024 bytes"));
    }
    Ok(value)
}
