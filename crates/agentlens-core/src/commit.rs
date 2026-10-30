//! The issue a `git commit -m` command refers to.
//!
//! Adapters pass the shell command of a tool call; only the issue number leaves this module,
//! never the message. The shell parsing is deliberately small (quotes, `$(…)`, heredocs inside a
//! substitution, `&&`/`;`/`|` between commands): a command it does not understand yields `None`.

/// Words in front of `#n` that refer to an issue: GitHub's closing keywords and `refs`.
const KEYWORDS: [&str; 11] = [
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved", "ref",
    "refs",
];

/// The issue the message of a `git commit` in `command` refers to: the first `Closes #n`,
/// `Refs #n` (or another closing keyword) or `(#n)`. Only `-m` and `--message` count; a message
/// from a file (`-F`) is not read.
pub fn commit_issue(command: &str) -> Option<u32> {
    commands(command)
        .iter()
        .flat_map(|words| messages(words))
        .find_map(issue_in)
}

/// The commands of a shell line as words, quotes removed.
fn commands(line: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = line.chars().collect();
    let mut commands = Vec::new();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' => {
                in_word = true;
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    word.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            '"' => {
                in_word = true;
                i = double_quoted(&chars, i + 1, &mut word);
            }
            '$' if chars.get(i + 1) == Some(&'(') => {
                in_word = true;
                i = substitution(&chars, i + 2, &mut word);
            }
            '\\' => {
                in_word = true;
                if let Some(&c) = chars.get(i + 1).filter(|c| **c != '\n') {
                    word.push(c);
                }
                i += 2;
            }
            c if c.is_whitespace() || matches!(c, ';' | '&' | '|') => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
                if !matches!(c, ' ' | '\t' | '\r') && !words.is_empty() {
                    commands.push(std::mem::take(&mut words));
                }
                i += 1;
            }
            c => {
                in_word = true;
                word.push(c);
                i += 1;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    if !words.is_empty() {
        commands.push(words);
    }
    commands
}

/// Reads a double-quoted string from `i` (after the opening quote); returns the index after it.
fn double_quoted(chars: &[char], mut i: usize, word: &mut String) -> usize {
    while i < chars.len() {
        match chars[i] {
            '"' => return i + 1,
            '\\' if chars
                .get(i + 1)
                .is_some_and(|c| matches!(c, '"' | '\\' | '$' | '`')) =>
            {
                word.push(chars[i + 1]);
                i += 2;
            }
            '$' if chars.get(i + 1) == Some(&'(') => i = substitution(chars, i + 2, word),
            c => {
                word.push(c);
                i += 1;
            }
        }
    }
    i
}

/// Copies a `$(…)` from `i` (after `$(`) as raw text, heredoc bodies included; returns the index
/// after the closing parenthesis.
fn substitution(chars: &[char], mut i: usize, word: &mut String) -> usize {
    let mut depth = 1;
    while i < chars.len() {
        match chars[i] {
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
                word.push(')');
                i += 1;
            }
            '(' => {
                depth += 1;
                word.push('(');
                i += 1;
            }
            q @ ('\'' | '"') => {
                word.push(q);
                i += 1;
                while i < chars.len() && chars[i] != q {
                    word.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            '<' if chars.get(i + 1) == Some(&'<') => i = heredoc(chars, i + 2, word),
            c => {
                word.push(c);
                i += 1;
            }
        }
    }
    i
}

/// Copies the body of a heredoc from `i` (after `<<`); returns the index after its end line.
fn heredoc(chars: &[char], mut i: usize, word: &mut String) -> usize {
    if chars.get(i) == Some(&'-') {
        i += 1;
    }
    while chars.get(i).is_some_and(|c| *c == ' ') {
        i += 1;
    }
    let quote = chars.get(i).copied().filter(|c| matches!(c, '\'' | '"'));
    if quote.is_some() {
        i += 1;
    }
    let mut delimiter = String::new();
    while let Some(&c) = chars.get(i) {
        if Some(c) == quote || c.is_whitespace() || c == ')' {
            break;
        }
        delimiter.push(c);
        i += 1;
    }
    if quote.is_some() {
        i += 1;
    }
    while i < chars.len() && chars[i] != '\n' {
        i += 1;
    }
    i += 1;
    while i < chars.len() {
        let end = chars[i..]
            .iter()
            .position(|c| *c == '\n')
            .map_or(chars.len(), |n| i + n);
        let line: String = chars[i..end].iter().collect();
        i = end + 1;
        if line.trim() == delimiter {
            return i.min(chars.len());
        }
        word.push_str(&line);
        word.push('\n');
    }
    i.min(chars.len())
}

/// The `-m` / `--message` values of a `git commit` command; empty for any other command.
fn messages(words: &[String]) -> Vec<&str> {
    let mut it = words
        .iter()
        .map(String::as_str)
        .skip_while(|w| w.contains('=') && !w.starts_with('-'));
    if !it.next().is_some_and(|w| w == "git" || w.ends_with("/git")) {
        return Vec::new();
    }
    loop {
        match it.next() {
            Some("commit") => break,
            Some("-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace") => {
                it.next();
            }
            Some(w) if w.starts_with('-') => {}
            _ => return Vec::new(),
        }
    }
    let mut out = Vec::new();
    while let Some(w) = it.next() {
        if w == "--" {
            break;
        }
        if let Some(m) = w.strip_prefix("--message=") {
            out.push(m);
        } else if w == "--message" {
            out.extend(it.next());
        } else if let Some(flags) = w.strip_prefix('-').filter(|f| !f.starts_with('-')) {
            // A cluster of short options; `m` and the options that take a value end it.
            for (n, c) in flags.char_indices() {
                if !matches!(c, 'm' | 'F' | 'C' | 'c' | 't') {
                    continue;
                }
                let rest = &flags[n + 1..];
                let value = if rest.is_empty() {
                    it.next()
                } else {
                    Some(rest)
                };
                if c == 'm' {
                    out.extend(value);
                }
                break;
            }
        }
    }
    out
}

/// The first `(#n)` or `<keyword> #n` in a message.
fn issue_in(message: &str) -> Option<u32> {
    for (i, _) in message.match_indices('#') {
        let digits = message[i + 1..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        let end = i + 1 + digits;
        if !(1..=7).contains(&digits)
            || message[end..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let before = &message[..i];
        let in_parens = before.ends_with('(') && message[end..].starts_with(')');
        let head = before.trim_end();
        let separated = head.len() < before.len() || head.ends_with(':');
        let word = head
            .trim_end_matches(':')
            .rsplit(|c: char| !c.is_alphabetic())
            .next()
            .unwrap_or_default();
        let after_keyword = separated && KEYWORDS.contains(&word.to_lowercase().as_str());
        if in_parens || after_keyword {
            return message[i + 1..end].parse().ok().filter(|n| *n > 0);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_issue_is_read_from_the_message() {
        for (command, want) in [
            (r#"git commit -m "feat: x (#502)""#, Some(502)),
            ("git commit -m 'fix: y' -m 'Closes #41'", Some(41)),
            (r#"git commit -am "chore: z" -m "Refs: #7""#, Some(7)),
            (r#"git commit --message="feat: a (#12)""#, Some(12)),
            (r#"git commit --message "fixes #13""#, Some(13)),
            (r#"git commit -m"feat: attached (#14)""#, Some(14)),
            (r#"git -C /tmp/repo commit -m "feat: x (#15)""#, Some(15)),
            (r#"git add a.rs && git commit -m "feat: x (#16)""#, Some(16)),
            (
                "git add a.rs && git commit -m \"$(cat <<'EOF'\nfeat: heredoc (#17)\n\nCo-Authored-By: x\nEOF\n)\"",
                Some(17),
            ),
            (
                r#"git commit -m "feat: first (#18)" -m "Closes #19""#,
                Some(18),
            ),
            (r#"git commit -m "feat: no issue""#, None),
            (r#"git commit -m "see issue #20""#, None),
            (r#"git commit -m "feat: (#abc)""#, None),
            ("git commit -F msg.txt", None),
            ("git commit -F - <<EOF\nfeat: from stdin (#21)\nEOF", None),
            (r#"git log --grep "(#22)""#, None),
            (r#"echo "git commit -m '(#23)'""#, None),
            (
                r#"git commit -m "feat: x" && gh pr create --body "Closes #24""#,
                None,
            ),
            (r#"git commit -C HEAD -m "(#25)""#, Some(25)),
            (r#"git commit -m"#, None),
            ("", None),
        ] {
            assert_eq!(commit_issue(command), want, "{command}");
        }
    }

    #[test]
    fn malformed_commands_do_not_panic() {
        for command in [
            "git commit -m \"unterminated (#1)",
            "git commit -m '$(",
            "git commit -m \"$(cat <<'EOF'",
            "git commit -m \"$(cat <<",
            "git commit -m \\",
            "git commit -m #",
            "git commit -m \"Closes #99999999\"",
        ] {
            let _ = commit_issue(command);
        }
        assert_eq!(commit_issue("git commit -m \"unterminated (#1)"), Some(1));
    }
}
