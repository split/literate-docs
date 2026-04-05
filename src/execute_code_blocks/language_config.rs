use markdown::mdast::Node;
use std::path::PathBuf;
use std::process::Command;

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

pub const EXECUTABLE_LANGUAGES: &[&str] = &[
    "sh",
    "bash",
    "shell",
    "python",
    "python3",
    "js",
    "javascript",
    "node",
    "ts",
    "typescript",
    "ruby",
    "perl",
    "php",
    "go",
    "rust",
];

const LANGUAGES: &[LanguageConfig] = &[
    LanguageConfig {
        aliases: &["sh", "bash", "shell"],
        commands: &[CommandTemplate {
            run: ExecCommand {
                tool: "/bin/sh",
                args: &["-c"],
                inline: true,
            },
            compile: None,
        }],
    },
    LanguageConfig {
        aliases: &["python", "python3"],
        commands: &[CommandTemplate {
            run: ExecCommand {
                tool: "python3",
                args: &["-c"],
                inline: true,
            },
            compile: None,
        }],
    },
    LanguageConfig {
        aliases: &["js", "javascript", "node"],
        commands: &[
            CommandTemplate {
                run: ExecCommand {
                    tool: "node_modules/.bin/node",
                    args: &["-e"],
                    inline: true,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "node_modules/.bin/bun",
                    args: &["-e"],
                    inline: true,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "node",
                    args: &["-e"],
                    inline: true,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "bun",
                    args: &["-e"],
                    inline: true,
                },
                compile: None,
            },
        ],
    },
    LanguageConfig {
        aliases: &["ruby"],
        commands: &[CommandTemplate {
            run: ExecCommand {
                tool: "ruby",
                args: &["-e"],
                inline: true,
            },
            compile: None,
        }],
    },
    LanguageConfig {
        aliases: &["perl"],
        commands: &[CommandTemplate {
            run: ExecCommand {
                tool: "perl",
                args: &["-e"],
                inline: true,
            },
            compile: None,
        }],
    },
    LanguageConfig {
        aliases: &["php"],
        commands: &[CommandTemplate {
            run: ExecCommand {
                tool: "php",
                args: &["-r"],
                inline: true,
            },
            compile: None,
        }],
    },
    LanguageConfig {
        aliases: &["go"],
        commands: &[CommandTemplate {
            run: ExecCommand {
                tool: "go",
                args: &["run", "{input}"],
                inline: false,
            },
            compile: None,
        }],
    },
    LanguageConfig {
        aliases: &["rust"],
        commands: &[CommandTemplate {
            run: ExecCommand {
                tool: "{output}",
                args: &[],
                inline: true,
            },
            compile: Some(ExecCommand {
                tool: "rustc",
                args: &["-o", "{output}", "{input}"],
                inline: false,
            }),
        }],
    },
    LanguageConfig {
        aliases: &["ts", "typescript"],
        commands: &[
            CommandTemplate {
                run: ExecCommand {
                    tool: "node_modules/.bin/ts-node",
                    args: &["-e"],
                    inline: true,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "node_modules/.bin/tsx",
                    args: &[],
                    inline: false,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "node_modules/.bin/bun",
                    args: &["-e"],
                    inline: true,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "node_modules/.bin/node",
                    args: &["--experimental-strip-types"],
                    inline: false,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "ts-node",
                    args: &["-e"],
                    inline: true,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "tsx",
                    args: &[],
                    inline: false,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "bun",
                    args: &["-e"],
                    inline: true,
                },
                compile: None,
            },
            CommandTemplate {
                run: ExecCommand {
                    tool: "node",
                    args: &["--experimental-strip-types"],
                    inline: false,
                },
                compile: None,
            },
        ],
    },
];

pub fn find_language(lang: &str) -> Option<&'static LanguageConfig> {
    LANGUAGES
        .iter()
        .find(|config| config.aliases.contains(&lang))
}

pub fn detect_tool(tool: &str) -> Option<PathBuf> {
    if Command::new(tool).arg("--version").output().is_ok() {
        Some(PathBuf::from(tool))
    } else {
        None
    }
}

pub fn is_executable(lang: &str) -> bool {
    LANGUAGES
        .iter()
        .any(|config| config.aliases.contains(&lang))
}
