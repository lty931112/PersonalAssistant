use std::path::PathBuf;

/// CLI 配置
pub struct Config {
    pub command: Command,
    pub config_path: PathBuf,
    pub verbose: bool,
}

/// CLI 命令
pub enum Command {
    Start,
    Query { prompt: String },
    Version,
}

/// 解析命令行参数
pub fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();

    let mut command = Command::Version;
    let mut config_path = PathBuf::from("config/default.toml");
    let mut verbose = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "start" => command = Command::Start,
            "query" => {
                if i + 1 < args.len() {
                    command = Command::Query {
                        prompt: args[i + 1].clone(),
                    };
                    i += 1;
                }
            }
            "version" | "--version" | "-v" => command = Command::Version,
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    config_path = PathBuf::from(&args[i + 1]);
                    i += 1;
                }
            }
            "--verbose" => verbose = true,
            _ => {}
        }
        i += 1;
    }

    Config {
        command,
        config_path,
        verbose,
    }
}
