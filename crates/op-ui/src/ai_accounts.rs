//! AI accounts (Preferences > AI Accounts): whether Claude and ChatGPT are signed in, and the
//! buttons to sign in or out. Both go through the vendors' own command line tools (`claude`,
//! `codex`), which keep the login themselves: this program never sees a password or a token.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Login {
    Unknown,
    Checking,
    SignedIn(String),
    SignedOut,
    /// The command line tool is not installed.
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Claude,
    ChatGpt,
}

pub struct Accounts {
    pub claude: Login,
    pub chatgpt: Login,
    rx: Option<mpsc::Receiver<(Provider, Login)>>,
    /// While a sign-in window is open the state is checked again every few seconds.
    watch_until: Option<Instant>,
    last: Option<Instant>,
}

impl Default for Accounts {
    fn default() -> Self {
        Accounts {
            claude: Login::Unknown,
            chatgpt: Login::Unknown,
            rx: None,
            watch_until: None,
            last: None,
        }
    }
}

fn find_on_path(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| names.iter().map(|n| dir.join(n)).find(|p| p.is_file()))
}

/// The folder `helper` beside the program's own folder (`<folder>\app\We edit.exe`).
pub fn helper_dir() -> Option<PathBuf> {
    std::env::var_os("WEEDIT_HELPER")
        .map(PathBuf::from)
        .or_else(|| Some(std::env::current_exe().ok()?.parent()?.parent()?.join("helper")))
}

/// `claude` from the PATH, else where its installer puts it.
pub fn claude_path() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    find_on_path(names).or_else(|| {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
        let p = PathBuf::from(home).join(".local").join("bin").join(names[0]);
        p.is_file().then_some(p)
    })
}

/// `codex` from the PATH, else the copy installed in `helper\codex` by the Install button.
pub fn codex_path() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["codex.cmd", "codex.exe"] } else { &["codex"] };
    find_on_path(names).or_else(|| {
        let bin = helper_dir()?.join("codex").join("node_modules").join(".bin");
        names.iter().map(|n| bin.join(n)).find(|p| p.is_file())
    })
}

fn quiet(cmd: &mut Command) -> &mut Command {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Runs the tool with `args` (a `.cmd` shim needs the shell) and returns its exit state and text.
fn run(tool: &PathBuf, args: &[&str]) -> Option<(bool, String)> {
    let mut cmd = if tool.extension().is_some_and(|e| e == "cmd") {
        let mut c = Command::new("cmd");
        c.arg("/c").arg(tool);
        c
    } else {
        Command::new(tool)
    };
    let out = quiet(cmd.args(args)).output().ok()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Some((out.status.success(), text))
}

fn claude_state() -> Login {
    let Some(tool) = claude_path() else { return Login::Missing };
    match run(&tool, &["auth", "status"]) {
        Some((_, text)) => match serde_json::from_str::<serde_json::Value>(text.trim()) {
            Ok(v) if v["loggedIn"] == true => {
                let who = v["email"].as_str().or_else(|| v["account"].as_str()).unwrap_or("Claude");
                Login::SignedIn(who.to_string())
            }
            _ => Login::SignedOut,
        },
        None => Login::SignedOut,
    }
}

fn chatgpt_state() -> Login {
    let Some(tool) = codex_path() else { return Login::Missing };
    match run(&tool, &["login", "status"]) {
        Some((true, text)) if text.to_lowercase().contains("logged in") => {
            Login::SignedIn(text.lines().next().unwrap_or("ChatGPT").trim().to_string())
        }
        _ => Login::SignedOut,
    }
}

/// A console window that stays open, running `command` (the tools ask for a browser login).
fn console(title: &str, command: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `start` opens a new window; the doubled quotes keep paths with spaces together
        Command::new("cmd")
            .raw_arg(format!("/c start \"{title}\" cmd /k \"{command}\""))
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = (title, command);
        Err(std::io::Error::other("not supported on this system"))
    }
}

impl Accounts {
    /// Checks both accounts in the background.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        if self.rx.is_some() {
            return;
        }
        self.claude = Login::Checking;
        self.chatgpt = Login::Checking;
        self.last = Some(Instant::now());
        let (tx, rx) = mpsc::channel();
        for (which, check) in [(Provider::Claude, claude_state as fn() -> Login), (Provider::ChatGpt, chatgpt_state)] {
            let (tx, ctx) = (tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send((which, check()));
                ctx.request_repaint();
            });
        }
        self.rx = Some(rx);
    }

    /// Takes finished checks and, while a sign-in window is open, asks again from time to time.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.rx {
            while let Ok((which, state)) = rx.try_recv() {
                match which {
                    Provider::Claude => self.claude = state,
                    Provider::ChatGpt => self.chatgpt = state,
                }
            }
            if !matches!(self.claude, Login::Checking) && !matches!(self.chatgpt, Login::Checking) {
                self.rx = None;
            }
        }
        if self.claude == Login::Unknown && self.chatgpt == Login::Unknown && self.rx.is_none() {
            self.refresh(ctx);
        }
        if let Some(until) = self.watch_until {
            if Instant::now() > until {
                self.watch_until = None;
            } else {
                if self.rx.is_none() && self.last.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
                    self.refresh(ctx);
                }
                ctx.request_repaint_after(Duration::from_secs(1));
            }
        }
    }

    pub fn sign_in(&mut self, which: Provider) -> Result<(), String> {
        let tool = match which {
            Provider::Claude => claude_path(),
            Provider::ChatGpt => codex_path(),
        }
        .ok_or("The tool is not installed")?;
        let (title, args) = match which {
            Provider::Claude => ("Claude", "auth login"),
            Provider::ChatGpt => ("ChatGPT", "login"),
        };
        console(title, &format!("\"{}\" {args}", tool.display())).map_err(|e| e.to_string())?;
        self.watch_until = Some(Instant::now() + Duration::from_secs(300));
        Ok(())
    }

    pub fn sign_out(&mut self, which: Provider, ctx: &egui::Context) {
        let (tool, args): (_, &[&str]) = match which {
            Provider::Claude => (claude_path(), &["auth", "logout"]),
            Provider::ChatGpt => (codex_path(), &["logout"]),
        };
        if let Some(tool) = tool {
            let _ = run(&tool, args);
        }
        self.rx = None;
        self.refresh(ctx);
    }

    /// Installs Codex (OpenAI's command line tool, which signs in with a ChatGPT account) next
    /// to the program, in `helper\codex`, in a window that shows what it does.
    pub fn install_codex(&mut self) -> Result<(), String> {
        let dir = helper_dir().ok_or("The helper folder is unknown")?.join("codex");
        console("Install Codex", &format!("npm install --prefix \"{}\" @openai/codex", dir.display()))
            .map_err(|e| e.to_string())?;
        self.watch_until = Some(Instant::now() + Duration::from_secs(600));
        Ok(())
    }
}
