/// JSON with comments and trailing commas, as plain JSON. What is inside a
/// string is left alone, so a `//` in a URL survives.
pub(crate) fn relax(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                while let Some(c) = chars.next() {
                    out.push(c);
                    match c {
                        '\\' => out.extend(chars.next()),
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                // The newline stays, so an error still names the right line.
                while chars.next_if(|c| *c != '\n').is_some() {}
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for c in chars.by_ref() {
                    if previous == '*' && c == '/' {
                        break;
                    }
                    previous = c;
                }
            }
            '}' | ']' => {
                // A comma with only whitespace between it and the bracket
                // that closes its list.
                let kept = out.trim_end().len();
                if out[..kept].ends_with(',') {
                    out.remove(kept - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}
