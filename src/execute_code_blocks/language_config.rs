use markdown::mdast::Node;

use super::default_language_config::is_executable;

#[derive(Debug, Clone)]
pub struct ExecutableCodeBlock {
    pub lang: String,
    pub code: String,
    pub hidden: bool,
}

impl TryFrom<&Node> for ExecutableCodeBlock {
    type Error = ();

    fn try_from(node: &Node) -> Result<Self, Self::Error> {
        match node {
            Node::Code(code) => {
                let lang = code.lang.as_deref().ok_or(())?;
                if is_executable(lang)
                    && code
                        .meta
                        .as_deref()
                        .map(|m| m.split_whitespace().any(|t| t == "exec"))
                        .unwrap_or(false)
                {
                    Ok(ExecutableCodeBlock {
                        lang: lang.to_string(),
                        code: code.value.clone(),
                        hidden: false,
                    })
                } else {
                    Err(())
                }
            }
            Node::Html(html) => {
                if let Some((lang, code)) = parse_hidden_exec_comment(&html.value) {
                    Ok(ExecutableCodeBlock {
                        lang,
                        code,
                        hidden: true,
                    })
                } else {
                    Err(())
                }
            }
            _ => Err(()),
        }
    }
}

pub fn is_executable_code_node(node: &Node) -> bool {
    match node {
        Node::Code(c) => {
            let lang = c.lang.as_deref().unwrap_or("");
            is_executable(lang)
                && c.meta
                    .as_deref()
                    .map(|m| m.split_whitespace().any(|t| t == "exec"))
                    .unwrap_or(false)
        }
        _ => false,
    }
}

pub fn is_executable_node(node: &Node) -> bool {
    is_executable_code_node(node) || is_hidden_executable_comment(node).is_some()
}

pub fn is_hidden_executable_comment(node: &Node) -> Option<(String, String)> {
    match node {
        Node::Html(h) => parse_hidden_exec_comment(&h.value),
        _ => None,
    }
}

fn parse_hidden_exec_comment(comment: &str) -> Option<(String, String)> {
    let trimmed = comment.trim();
    if !trimmed.starts_with("<!--") || !trimmed.ends_with("-->") {
        return None;
    }
    let inner = &trimmed[4..trimmed.len() - 3].trim();
    let parts: Vec<&str> = inner.splitn(2, " exec: ").collect();
    if parts.len() != 2 {
        return None;
    }
    let lang = parts[0].trim();
    let code = parts[1].trim();
    if lang.is_empty() || code.is_empty() {
        return None;
    }
    if !is_executable(lang) {
        return None;
    }
    Some((lang.to_string(), code.to_string()))
}

pub struct ExecCommand {
    pub tool: &'static str,
    pub args: &'static [&'static str],
    pub inline: bool,
}

pub struct CommandTemplate {
    pub run: ExecCommand,
    pub compile: Option<ExecCommand>,
}

pub struct LanguageConfig {
    pub aliases: &'static [&'static str],
    pub commands: &'static [CommandTemplate],
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown::{to_mdast, ParseOptions};

    fn parse(input: &str) -> Vec<Node> {
        let ast = to_mdast(input, &ParseOptions::default()).unwrap();
        ast.children().unwrap().to_vec()
    }

    #[test]
    fn test_executable_code_block_from_code_with_exec() {
        let nodes = parse("```sh exec\necho hello\n```");
        let result = ExecutableCodeBlock::try_from(&nodes[0]);
        assert!(result.is_ok());
        let block = result.unwrap();
        assert_eq!(block.lang, "sh");
        assert_eq!(block.code, "echo hello");
        assert!(!block.hidden);
    }

    #[test]
    fn test_executable_code_block_from_code_without_exec() {
        let nodes = parse("```sh\ncode\n```");
        let result = ExecutableCodeBlock::try_from(&nodes[0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_executable_code_block_from_code_unknown_lang() {
        let nodes = parse("```mermaid exec\ngraph TD; A-->B;\n```");
        let result = ExecutableCodeBlock::try_from(&nodes[0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_executable_code_block_from_hidden_comment() {
        let nodes = parse("<!-- sh exec: echo hello -->");
        let result = ExecutableCodeBlock::try_from(&nodes[0]);
        assert!(result.is_ok());
        let block = result.unwrap();
        assert_eq!(block.lang, "sh");
        assert_eq!(block.code, "echo hello");
        assert!(block.hidden);
    }

    #[test]
    fn test_executable_code_block_from_non_exec_comment() {
        let nodes = parse("<!-- some comment -->");
        let result = ExecutableCodeBlock::try_from(&nodes[0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_executable_code_block_from_invalid_comment() {
        let nodes = parse("<!-- sh: echo hello -->");
        let result = ExecutableCodeBlock::try_from(&nodes[0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_executable_code_node_with_exec() {
        let nodes = parse("```sh exec\ncode\n```");
        assert!(is_executable_code_node(&nodes[0]));
    }

    #[test]
    fn test_is_executable_code_node_without_exec() {
        let nodes = parse("```sh\ncode\n```");
        assert!(!is_executable_code_node(&nodes[0]));
    }

    #[test]
    fn test_is_hidden_executable_comment_valid() {
        let nodes = parse("<!-- sh exec: echo hello -->");
        let result = is_hidden_executable_comment(&nodes[0]);
        assert!(result.is_some());
        let (lang, code) = result.unwrap();
        assert_eq!(lang, "sh");
        assert_eq!(code, "echo hello");
    }

    #[test]
    fn test_is_hidden_executable_comment_invalid() {
        let nodes = parse("<!-- not exec: echo hello -->");
        let result = is_hidden_executable_comment(&nodes[0]);
        assert!(result.is_none());
    }

    #[test]
    fn test_is_executable_node_code() {
        let nodes = parse("```sh exec\ncode\n```");
        assert!(is_executable_node(&nodes[0]));
    }

    #[test]
    fn test_is_executable_node_hidden_comment() {
        let nodes = parse("<!-- sh exec: echo hello -->");
        assert!(is_executable_node(&nodes[0]));
    }
}
