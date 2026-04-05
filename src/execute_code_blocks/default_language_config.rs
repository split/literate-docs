use super::language_config::{
    find_language_in, is_executable_in, CommandTemplate, ExecCommand, LanguageConfig,
};

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

pub const LANGUAGES_SLICE: &[LanguageConfig] = LANGUAGES;

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

pub fn find_language(lang: &str) -> Option<&'static LanguageConfig> {
    find_language_in(LANGUAGES, lang)
}

pub fn is_executable(lang: &str) -> bool {
    is_executable_in(LANGUAGES, lang)
}
