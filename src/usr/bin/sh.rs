use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::sys::fs::{InodeType, VFS};

static IS_ROOT: AtomicBool = AtomicBool::new(false);
static AWAITING_SU_AUTH: AtomicBool = AtomicBool::new(false);
static BG_JOB_COUNTER: AtomicUsize = AtomicUsize::new(1);

pub fn main() -> ! {
    let mut term = crate::drivers::wm::get_terminal_descriptor();
    let mut cwd_inode = 3;
    let mut cwd_path = String::from("/home");
    let mut vars: BTreeMap<String, String> = BTreeMap::new();
    let mut aliases: BTreeMap<String, String> = BTreeMap::new();
    let mut history: Vec<String> = Vec::new();

    vars.insert(String::from("OSTYPE"), String::from("redKiteOS-zsh-analog"));
    vars.insert(String::from("SHELL"), String::from("/bin/sh"));
    vars.insert(String::from("USER"), String::from("senpai"));
    vars.insert(String::from("LOGNAME"), String::from("senpai"));
    vars.insert(String::from("UID"), String::from("1000"));
    vars.insert(String::from("GID"), String::from("1000"));
    vars.insert(String::from("HOME"), String::from("/home"));
    vars.insert(String::from("PWD"), String::from("/home"));
    vars.insert(String::from("PATH"), String::from("/bin:/usr/bin"));
    vars.insert(String::from("TERM"), String::from("kite-vga-color"));
    vars.insert(String::from("STDOUT"), String::new());
    vars.insert(String::from("WINDOW_ID"), String::from("0"));
    vars.insert(String::from("0"), String::from("/bin/sh"));
    vars.insert(String::from("?"), String::from("0"));
    vars.insert(String::from("$"), String::from("1"));
    vars.insert(String::from("!"), String::from("0"));
    vars.insert(String::from("PS1"), String::from("\\e[36m\\u@\\h\\e[0m:\\e[35m\\w\\e[0m\\$ "));

    aliases.insert(String::from("ll"), String::from("ls -l -a"));
    aliases.insert(String::from("la"), String::from("ls -a"));
    aliases.insert(String::from("cls"), String::from("clear"));

    term_println(&mut term, "redKiteOS KiteScript Shell");
    term_println(&mut term, "Type 'help' for builtins, b-baka!");

    run_script_file(".initrc", &mut cwd_inode, &mut cwd_path, &mut vars, &mut aliases, &mut history, &mut term);

    loop {
        let is_root = IS_ROOT.load(Ordering::Relaxed);
        let ps1_fmt = vars.get("PS1").cloned().unwrap_or_else(|| String::from("\\u@\\h:\\w\\$ "));
        let prompt = format_prompt(&ps1_fmt, &vars, &cwd_path, is_root);
        term_print(&mut term, &prompt);

        let cmd_line = term.read_line();
        let trimmed = clean_string(&cmd_line);

        if !trimmed.is_empty() {
            history.push(String::from(trimmed));
        }

        if AWAITING_SU_AUTH.load(Ordering::Relaxed) {
            match trimmed {
                "yes" => {
                    term_println(&mut term, "[pam_auth]: Success. Ring 0 / UID=0 (God Mode) granted. May the compiler have mercy on your soul, Onii-chan~");
                    IS_ROOT.store(true, Ordering::Relaxed);
                    vars.insert(String::from("USER"), String::from("root"));
                    vars.insert(String::from("LOGNAME"), String::from("root"));
                    vars.insert(String::from("UID"), String::from("0"));
                    vars.insert(String::from("GID"), String::from("0"));
                    vars.insert(String::from("HOME"), String::from("/root"));
                    vars.insert(String::from("?"), String::from("0"));
                }
                "no" => {
                    term_println(&mut term, "[pam_auth]: Permission denied (EACCES). Smart move, b-baka! Ring 3 sandbox kept cozy.");
                    vars.insert(String::from("?"), String::from("1"));
                }
                _ => {
                    term_println(&mut term, "[pam_auth]: Nani?! Authentication failure (EINVAL). Unknown credential.");
                    vars.insert(String::from("?"), String::from("1"));
                }
            }
            AWAITING_SU_AUTH.store(false, Ordering::Relaxed);
            continue;
        }

        if trimmed.is_empty() { continue; }

        execute_script_line(trimmed, &mut cwd_inode, &mut cwd_path, &mut vars, &mut aliases, &mut history, &mut term);
    }
}

fn format_prompt(ps1: &str, vars: &BTreeMap<String, String>, cwd_path: &str, is_root: bool) -> String {
    let mut out = String::new();
    let mut chars = ps1.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(nxt) = chars.next() {
                match nxt {
                    'u' => out.push_str(vars.get("USER").map(|s| s.as_str()).unwrap_or("user")),
                    'h' => out.push_str("redkite"),
                    'w' => out.push_str(cwd_path),
                    '$' => out.push(if is_root { '#' } else { '$' }),
                    'e' => out.push('\x1b'),
                    _ => { out.push('\\'); out.push(nxt); }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn term_print(term: &mut crate::drivers::wm::TerminalDescriptor, s: &str) {
    term.write_str(s);
}

fn term_println(term: &mut crate::drivers::wm::TerminalDescriptor, s: &str) {
    term.write_str(s);
    term.write_str("\n");
}

fn strip_quotes(s: &str) -> &str {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
        || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2)
    {
        &t[1..t.len() - 1]
    } else {
        t.trim_matches(|c| c == '"' || c == '\'')
    }
}

fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_double_quote = false;
    let mut in_single_quote = false;

    for c in s.chars() {
        match c {
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            c if c.is_whitespace() && !in_double_quote && !in_single_quote => {
                if !current.is_empty() {
                    words.push(current);
                    current = String::new();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn is_known_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "help" | "run" | "recv" | "read" | "window" | "sh" | "echo" | "cat" | "touch" | "mkdir"
        | "write" | "append" | "chmod" | "rm" | "clear" | "reboot" | "pwd"
        | "cd" | "ls" | "ping" | "ifconfig" | "env" | "export" | "unset"
        | "test" | "[" | "uname" | "whoami" | "id" | "su" | "sudo" | "kdev"
        | "user" | "exit" | "gui" | "desktop" | "beep" | "music" | "ghoul"
        | "format" | "mkfs" | "panic" | "divzero" | "gpf" | "overflow"
        | "true" | "false" | "alias" | "history"
    ) || cmd.starts_with('/') || cmd.starts_with("./")
}

fn find_op(line: &str, op: &str) -> Option<usize> {
    let mut in_d = false;
    let mut in_s = false;
    let b = line.as_bytes();
    let ob = op.as_bytes();
    if b.len() < ob.len() { return None; }

    for i in 0..=b.len() - ob.len() {
        match b[i] {
            b'"' if !in_s => in_d = !in_d,
            b'\'' if !in_d => in_s = !in_s,
            _ => {
                if !in_d && !in_s {
                    let mut ok = true;
                    for j in 0..ob.len() {
                        if b[i+j] != ob[j] { ok = false; break; }
                    }
                    if ok {
                        if op == ">" && i + 1 < b.len() && (b[i+1] == b'=' || b[i+1] == b'>') { continue; }
                        if op == "<" && i + 1 < b.len() && (b[i+1] == b'=' || b[i+1] == b'<') { continue; }
                        if op == "|" && i + 1 < b.len() && b[i+1] == b'|' { continue; }
                        if op == "&" && i + 1 < b.len() && b[i+1] == b'&' { continue; }
                        return Some(i);
                    }
                }
            }
        }
    }
    None
}

fn parse_assignment(line: &str) -> Option<(&str, &str)> {
    if let Some(eq_pos) = line.find('=') {
        if line[eq_pos + 1..].starts_with('=') {
            return None;
        }
        let pre_part = line[..eq_pos].trim();
        let mut pre_words = pre_part.split_whitespace();
        let var_part = match pre_words.next() {
            Some(w) => w.trim_matches(|c| c == '"' || c == '\''),
            None => return None,
        };
        if pre_words.next().is_some() {
            return None;
        }
        if is_known_command(var_part) {
            return None;
        }
        let val_part = line[eq_pos + 1..].trim();
        let clean_val = strip_quotes(val_part);
        if !var_part.is_empty() && var_part.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '[' || c == ']') {
            return Some((var_part, clean_val));
        }
    }
    None
}

fn execute_script_line(
    line: &str,
    cwd_inode: &mut usize,
    cwd_path: &mut String,
    vars: &mut BTreeMap<String, String>,
    aliases: &mut BTreeMap<String, String>,
    history: &mut Vec<String>,
    term: &mut crate::drivers::wm::TerminalDescriptor,
) {
    let mut trimmed = line.trim();

    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        trimmed = trimmed[1..trimmed.len() - 1].trim();
    } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        trimmed = trimmed.trim_matches(|c| c == '"' || c == '\'').trim();
    }

    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("#!") {
        return;
    }

    if trimmed.starts_with("if ") {
        execute_if_statement(trimmed, cwd_inode, cwd_path, vars, aliases, history, term);
        return;
    }

    if trimmed.starts_with("while ") {
        execute_while_loop(trimmed, cwd_inode, cwd_path, vars, aliases, history, term);
        return;
    }

    if trimmed.starts_with("for ") {
        execute_for_loop(trimmed, cwd_inode, cwd_path, vars, aliases, history, term);
        return;
    }

    if let Some(pos) = find_op(trimmed, "||") {
        let left = trimmed[..pos].trim();
        let right = trimmed[pos + 2..].trim();
        execute_script_line(left, cwd_inode, cwd_path, vars, aliases, history, term);
        let exit_code = vars.get("?").map(|s| s.as_str()).unwrap_or("0");
        if exit_code != "0" {
            execute_script_line(right, cwd_inode, cwd_path, vars, aliases, history, term);
        }
        return;
    }

    if let Some(pos) = find_op(trimmed, "&&") {
        let left = trimmed[..pos].trim();
        let right = trimmed[pos + 2..].trim();
        execute_script_line(left, cwd_inode, cwd_path, vars, aliases, history, term);
        let exit_code = vars.get("?").map(|s| s.as_str()).unwrap_or("0");
        if exit_code == "0" {
            execute_script_line(right, cwd_inode, cwd_path, vars, aliases, history, term);
        }
        return;
    }

    if let Some(pipe_pos) = find_op(trimmed, "|") {
        let left_cmd = trimmed[..pipe_pos].trim();
        let right_cmd = trimmed[pipe_pos + 1..].trim();
        if !left_cmd.is_empty() && !right_cmd.is_empty() {
            let captured = execute_pipeline_left(left_cmd, cwd_inode, cwd_path, vars, aliases, history, term);
            vars.insert(String::from("STDOUT"), captured.clone());
            let combined = if captured.is_empty() {
                String::from(right_cmd)
            } else {
                format!("{} {}", right_cmd, captured.trim())
            };
            execute_script_line(&combined, cwd_inode, cwd_path, vars, aliases, history, term);
            return;
        }
    }

    if let Some(pos) = find_op(trimmed, ">>") {
        let left_cmd = trimmed[..pos].trim();
        let right_file = strip_quotes(trimmed[pos + 2..].trim());
        if !left_cmd.is_empty() && !right_file.is_empty() {
            let captured = execute_pipeline_left(left_cmd, cwd_inode, cwd_path, vars, aliases, history, term);
            let is_root = IS_ROOT.load(Ordering::Relaxed);
            let (cur_uid, cur_gid): (u32, u32) = if is_root { (0, 0) } else { (1000, 1000) };
            let fs = VFS.lock();
            let start_ino = if right_file.starts_with('/') { 0 } else { *cwd_inode };
            let ino_res = fs.resolve_path(start_ino, right_file).or_else(|_| fs.create_file(start_ino, right_file, cur_uid, cur_gid));
            match ino_res {
                Ok(ino) => {
                    let mut data = Vec::from(captured.as_bytes());
                    if !data.ends_with(&[b'\n']) { data.push(b'\n'); }
                    let _ = fs.append_inode_data(ino, &data);
                    vars.insert(String::from("?"), String::from("0"));
                }
                Err(_) => {
                    vars.insert(String::from("?"), String::from("1"));
                }
            }
            return;
        }
    }

    if let Some(pos) = find_op(trimmed, ">") {
        let left_cmd = trimmed[..pos].trim();
        let right_file = strip_quotes(trimmed[pos + 1..].trim());
        if !left_cmd.is_empty() && !right_file.is_empty() {
            let captured = execute_pipeline_left(left_cmd, cwd_inode, cwd_path, vars, aliases, history, term);
            let is_root = IS_ROOT.load(Ordering::Relaxed);
            let (cur_uid, cur_gid): (u32, u32) = if is_root { (0, 0) } else { (1000, 1000) };
            let fs = VFS.lock();
            let start_ino = if right_file.starts_with('/') { 0 } else { *cwd_inode };
            let ino_res = fs.resolve_path(start_ino, right_file).or_else(|_| fs.create_file(start_ino, right_file, cur_uid, cur_gid));
            match ino_res {
                Ok(ino) => {
                    let mut data = Vec::from(captured.as_bytes());
                    if !data.ends_with(&[b'\n']) { data.push(b'\n'); }
                    let _ = fs.write_inode_data(ino, &data);
                    vars.insert(String::from("?"), String::from("0"));
                }
                Err(_) => {
                    vars.insert(String::from("?"), String::from("1"));
                }
            }
            return;
        }
    }

    let is_background = if trimmed.ends_with('&') && !trimmed.ends_with("&&") {
        trimmed = trimmed[..trimmed.len() - 1].trim();
        true
    } else {
        false
    };

    if trimmed.is_empty() {
        return;
    }

    if is_background {
        let bg_line = String::from(trimmed);
        let bg_ino = *cwd_inode;
        let bg_path = cwd_path.clone();
        let mut bg_vars = vars.clone();
        let mut bg_aliases = aliases.clone();
        let mut bg_history = history.clone();
        let job_id = BG_JOB_COUNTER.fetch_add(1, Ordering::Relaxed);

        term_println(term, &format!("[{}] {} &", job_id, bg_line));
        vars.insert(String::from("!"), format!("{}", job_id));
        vars.insert(String::from("?"), String::from("0"));

        crate::kernel::task::spawn(move || {
            let mut ino = bg_ino;
            let mut path = bg_path;
            let mut bg_term = crate::drivers::wm::get_terminal_descriptor();
            execute_script_line(&bg_line, &mut ino, &mut path, &mut bg_vars, &mut bg_aliases, &mut bg_history, &mut bg_term);
        });
        return;
    }

    if let Some((var_name, val_expr)) = parse_assignment(trimmed) {
        let eval_res = evaluate_expr(val_expr, vars);
        vars.insert(String::from(var_name), eval_res);
        vars.insert(String::from("?"), String::from("0"));
        return;
    }

    let expanded = expand_vars(trimmed, vars);
    let mut words = split_words(&expanded);
    if words.is_empty() {
        return;
    }

    if let Some(alias_val) = aliases.get(&words[0]) {
        let mut exp_alias = alias_val.clone();
        for arg in &words[1..] {
            exp_alias.push(' ');
            exp_alias.push_str(arg);
        }
        words = split_words(&exp_alias);
        if words.is_empty() { return; }
    }

    let raw_cmd = &words[0];
    let args: Vec<&str> = words[1..].iter().map(|s| s.as_str()).collect();
    let is_root = IS_ROOT.load(Ordering::Relaxed);
    let (cur_uid, cur_gid): (u32, u32) = if is_root { (0, 0) } else { (1000, 1000) };

    let mut cmd_lower = String::with_capacity(raw_cmd.len());
    for c in raw_cmd.chars() {
        cmd_lower.push(c.to_ascii_lowercase());
    }

    match cmd_lower.as_str() {
        "help" => {
            term_println(term, "POSIX / UNIX Standard Shell Builtins (ZSH-Edition):");
            term_println(term, "  alias <name>=<val>   - define or list command aliases");
            term_println(term, "  history              - show command history");
            term_println(term, "  <c1> && <c2>         - logical AND chaining");
            term_println(term, "  <c1> || <c2>         - logical OR chaining");
            term_println(term, "  <c1> > <file>        - stdout redirection (overwrite)");
            term_println(term, "  <c1> >> <file>       - stdout redirection (append)");
            term_println(term, "  run <path> [args...] - execute binary executable");
            term_println(term, "  recv <path>          - receive binary stream from serial");
            term_println(term, "  read <var>           - read standard input line into variable");
            term_println(term, "  window <t> <w> <h>   - spawn graphical window via window manager");
            term_println(term, "  <cmd> &              - execute process in background");
            term_println(term, "  ls [-l] [-a], cd, pwd, cat, touch, mkdir [-p], rm [-r] [-f]");
            term_println(term, "  write <file> <text>  - write string to file");
            term_println(term, "  append <file> <text> - append string to file");
            term_println(term, "  chmod <octal> <file> - change file mode bits");
            term_println(term, "  echo <args...>       - write arguments to standard output");
            term_println(term, "  env, export, unset   - environment variable management");
            term_println(term, "  uname [-a]           - print system information");
            term_println(term, "  whoami, id           - print user and group identification");
            term_println(term, "  su, sudo, kdev       - elevate privileges to superuser (UID=0)");
            term_println(term, "  user, exit           - drop privileges or terminate session");
            term_println(term, "  test, [ <expr> ]     - evaluate condition expression");
            term_println(term, "  true, false          - return 0 or non-zero status code");
            term_println(term, "  ifconfig             - network interface configuration");
            term_println(term, "  ping <target_ip>     - send ICMP ECHO_REQUEST packets");
            term_println(term, "  sh <script_file>     - execute script file");
            term_println(term, "  clear, reboot        - clear terminal / reboot machine");
            term_println(term, "  beep, music          - audio subsystem diagnostics");
            term_println(term, "  ghoul                - compute 1000 - 7 sequence");
            if is_root {
                term_println(term, "");
                term_println(term, "Superuser (UID=0) Operations:");
                term_println(term, "  format, mkfs         - format filesystem partition");
                term_println(term, "  panic                - trigger intentional kernel panic");
                term_println(term, "  divzero              - provoke divide error exception (#DE)");
                term_println(term, "  gpf                  - provoke general protection fault (#GP)");
                term_println(term, "  overflow             - provoke stack overflow double fault (#DF)");
            }
            vars.insert(String::from("?"), String::from("0"));
        }
        "alias" => {
            if args.is_empty() {
                for (k, v) in aliases.iter() {
                    term_println(term, &format!("alias {}='{}'", k, v));
                }
                vars.insert(String::from("?"), String::from("0"));
            } else {
                for arg in args {
                    if let Some(eq_pos) = arg.find('=') {
                        let k = arg[..eq_pos].trim();
                        let v = strip_quotes(arg[eq_pos + 1..].trim());
                        aliases.insert(String::from(k), String::from(v));
                    }
                }
                vars.insert(String::from("?"), String::from("0"));
            }
        }
        "history" => {
            let mut i = 1;
            for h in history.iter() {
                term_println(term, &format!("{:>5}  {}", i, h));
                i += 1;
            }
            vars.insert(String::from("?"), String::from("0"));
        }
        "window" => {
            if args.len() >= 3 {
                let title = strip_quotes(args[0]);
                let width = strip_quotes(args[1]).parse::<usize>().unwrap_or(300);
                let height = strip_quotes(args[2]).parse::<usize>().unwrap_or(200);

                match crate::drivers::wm::create_window(title, width, height) {
                    Some(win_id) => {
                        term_println(term, &format!("[wm]: window '{}' created successfully (id: {}). Sugoi!", title, win_id));
                        vars.insert(String::from("WINDOW_ID"), format!("{}", win_id));
                        vars.insert(String::from("?"), String::from("0"));
                    }
                    None => {
                        term_println(term, &format!("window: failed to create window '{}'", title));
                        vars.insert(String::from("?"), String::from("1"));
                    }
                }
            } else {
                term_println(term, "window: usage: window <title> <width> <height>, b-baka!");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "read" => {
            if let Some(&var_name_raw) = args.get(0) {
                let var_name = strip_quotes(var_name_raw);
                if !var_name.is_empty() && var_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '[' || c == ']') {
                    let input = term.read_line();
                    let clean_input = clean_string(&input);
                    vars.insert(String::from(var_name), String::from(clean_input));
                    vars.insert(String::from("?"), String::from("0"));
                } else {
                    term_println(term, &format!("read: '{}': not a valid identifier, baka!", var_name));
                    vars.insert(String::from("?"), String::from("1"));
                }
            } else {
                term_println(term, "read: missing variable name operand");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "recv" => {
            if let Some(&name) = args.get(0) {
                let clean_name = strip_quotes(name);
                term_println(term, "Listening on serial port for transmission...");
                crate::drivers::gfx::compositor_tick();

                let mut payload = Vec::new();
                let mut idle_spins = 0;
                let start_tick = crate::kernel::timer::ticks();
                let mut aborted = false;
                let mut timed_out = false;

                loop {
                    crate::drivers::gfx::compositor_tick();

                    if let Some(c) = term.poll_char() {
                        if c == '\x1b' || c == 'q' || c == 'Q' {
                            aborted = true;
                            break;
                        }
                    }

                    if let Some(b) = crate::arch::x86_64::serial::read_byte() {
                        payload.push(b);
                        idle_spins = 0;
                    } else if !payload.is_empty() {
                        idle_spins += 1;
                        if idle_spins > 2_000_000 {
                            break;
                        }
                    } else if crate::kernel::timer::ticks().saturating_sub(start_tick) >= 1000 {
                        timed_out = true;
                        break;
                    }
                }

                if aborted {
                    term_println(term, "recv: aborted by user, b-baka!");
                    vars.insert(String::from("?"), String::from("130"));
                } else if timed_out {
                    term_println(term, "recv: connection timed out");
                    vars.insert(String::from("?"), String::from("1"));
                } else if payload.is_empty() {
                    term_println(term, "recv: transmission timeout on serial port. Gomen nasai!");
                    vars.insert(String::from("?"), String::from("1"));
                } else {
                    let fs = VFS.lock();
                    let start_ino = if clean_name.starts_with('/') { 0 } else { *cwd_inode };
                    let file_res = fs.resolve_path(start_ino, clean_name)
                        .or_else(|_| fs.create_executable_file(start_ino, clean_name, 0o755, cur_uid, cur_gid));

                    match file_res {
                        Ok(ino) => {
                            match fs.write_inode_data(ino, &payload) {
                                Ok(()) => {
                                    term_println(term, &format!("Received {} bytes. Written to '{}' [mode 0755] (Sugoi!)", payload.len(), clean_name));
                                    vars.insert(String::from("?"), String::from("0"));
                                }
                                Err(e) => {
                                    term_println(term, &format!("recv: write error: {}", e.as_str()));
                                    vars.insert(String::from("?"), String::from("1"));
                                }
                            }
                        }
                        Err(e) => {
                            term_println(term, &format!("recv: failed to create destination file: {}", e.as_str()));
                            vars.insert(String::from("?"), String::from("1"));
                        }
                    }
                }
            } else {
                term_println(term, "recv: missing destination path operand, b-baka!");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "run" => {
            if let Some(&prog_to_run) = args.get(0) {
                let clean_prog = strip_quotes(prog_to_run);
                let sub_args = if args.len() > 1 { &args[1..] } else { &[][..] };
                let start_ino = if clean_prog.starts_with('/') { 0 } else { *cwd_inode };
                let fs = VFS.lock();
                let exists = fs.resolve_path(start_ino, clean_prog).is_ok()
                    || (!clean_prog.starts_with('/') && fs.resolve_path(0, &format!("/bin/{}", clean_prog)).is_ok());
                drop(fs);

                if exists {
                    let ok = crate::usr::bin::execute_binary(start_ino, clean_prog, sub_args);
                    vars.insert(String::from("?"), String::from(if ok { "0" } else { "1" }));
                } else {
                    term_println(term, &format!("run: cannot execute '{}': No such file or directory (ENOENT). Baka!", clean_prog));
                    vars.insert(String::from("?"), String::from("127"));
                }
            } else {
                term_println(term, "run: missing program operand");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "true" => {
            vars.insert(String::from("?"), String::from("0"));
        }
        "false" => {
            vars.insert(String::from("?"), String::from("1"));
        }
        "uname" => {
            let out = if args.iter().any(|&a| a == "-a") {
                "redKiteOS localhost 4.0-zsh redKiteOS-posix x86_64 GNU/Anime"
            } else {
                "redKiteOS"
            };
            term_println(term, out);
            vars.insert(String::from("STDOUT"), String::from(out));
            vars.insert(String::from("?"), String::from("0"));
        }
        "whoami" => {
            let u = vars.get("USER").cloned().unwrap_or_else(|| String::from("unknown"));
            term_println(term, &u);
            vars.insert(String::from("STDOUT"), u);
            vars.insert(String::from("?"), String::from("0"));
        }
        "id" => {
            let out = if is_root {
                "uid=0(root) gid=0(root) groups=0(root)"
            } else {
                "uid=1000(senpai) gid=1000(senpai) groups=1000(senpai),4(adm),27(sudo)"
            };
            term_println(term, out);
            vars.insert(String::from("STDOUT"), String::from(out));
            vars.insert(String::from("?"), String::from("0"));
        }
        "su" | "sudo" | "kdev" => {
            if is_root {
                term_println(term, "su: user is already in Ring 0 God Mode, baka senpai!");
                vars.insert(String::from("?"), String::from("0"));
            } else {
                term_println(term, "[pam_auth]: Escalating privileges to UID=0 (Ring 0 God Mode).");
                term_println(term, "[WARNING]: Matte! Leaving the safe Ring 3 sandbox. One mistake and kernel goes boom!");
                term_print(term, "Are you sure you want to awaken the superuser dragon? (yes/no): ");
                AWAITING_SU_AUTH.store(true, Ordering::Relaxed);
            }
        }
        "user" => {
            if !is_root {
                term_println(term, "sh: user: Anta baka?! Session is already in Ring 3 userspace!");
                vars.insert(String::from("?"), String::from("1"));
            } else {
                term_println(term, "[pam_auth]: Relinquishing UID=0. Dropped to Ring 3 userspace. Yokatta~");
                IS_ROOT.store(false, Ordering::Relaxed);
                vars.insert(String::from("USER"), String::from("senpai"));
                vars.insert(String::from("LOGNAME"), String::from("senpai"));
                vars.insert(String::from("UID"), String::from("1000"));
                vars.insert(String::from("GID"), String::from("1000"));
                vars.insert(String::from("HOME"), String::from("/home"));
                vars.insert(String::from("?"), String::from("0"));
            }
        }
        "format" | "mkfs" => {
            if !is_root {
                term_println(term, "mkfs: Operation not permitted (EPERM). Superuser UID=0 required, b-baka!");
                vars.insert(String::from("?"), String::from("1"));
            } else {
                let _ = VFS.lock().format();
                term_println(term, "mkfs: filesystem formatted successfully. Subete ga kirei ni natta!");
                vars.insert(String::from("?"), String::from("0"));
            }
        }
        "panic" => {
            if !is_root {
                term_println(term, "panic: Operation not permitted (EPERM). Suicide commands require UID=0!");
                vars.insert(String::from("?"), String::from("1"));
            } else {
                panic!("Manual panic triggered by superuser: Sayonara, cruel world...");
            }
        }
        "divzero" => {
            if !is_root {
                term_println(term, "divzero: Operation not permitted (EPERM). Hardware crash tests require UID=0!");
                vars.insert(String::from("?"), String::from("1"));
            } else {
                unsafe {
                    core::arch::asm!("mov eax, 1", "xor ecx, ecx", "div ecx");
                }
            }
        }
        "gpf" => {
            if !is_root {
                term_println(term, "gpf: Operation not permitted (EPERM). Hardware crash tests require UID=0!");
                vars.insert(String::from("?"), String::from("1"));
            } else {
                unsafe {
                    core::arch::asm!(
                        "mov rax, 0x0000800000000000",
                        "mov rax, [rax]"
                    );
                }
            }
        }
        "overflow" => {
            if !is_root {
                term_println(term, "overflow: Operation not permitted (EPERM). Hardware crash tests require UID=0!");
                vars.insert(String::from("?"), String::from("1"));
            } else {
                #[allow(unconditional_recursion)]
                fn blow_stack() {
                    let _dummy = [0u8; 1024];
                    blow_stack();
                }
                blow_stack();
            }
        }
        "beep" => {
            crate::drivers::sound::beep();
            vars.insert(String::from("?"), String::from("0"));
        }
        "music" => {
            term_println(term, "Now playing: Anime Melodies (Megalovania synth)~");
            let notes: [(u32, u64); 10] = [
                (294, 3), (294, 3), (587, 6), (440, 6),
                (415, 6), (392, 6), (349, 6), (294, 3),
                (349, 3), (392, 5),
            ];
            for &(freq, dur) in notes.iter() {
                crate::drivers::sound::play_freq(freq);
                crate::kernel::timer::sleep_ticks(dur);
                crate::drivers::sound::stop();
                crate::kernel::timer::sleep_ticks(1);
            }
            vars.insert(String::from("?"), String::from("0"));
        }
        "ghoul" => {
            let mut val: i32 = 1000;
            while val >= 0 {
                let next = val - 7;
                term_println(term, &format!("{} - 7 = {}", val, next));
                val = next;
            }
            term_println(term, "Oshiete, oshiete yo... 1000 - 7 = -1. I am a ghoul.");
            vars.insert(String::from("?"), String::from("0"));
        }
        "ping" => {
            let target_str = args.get(0).copied().unwrap_or("10.0.2.2");
            let clean_target = strip_quotes(target_str);
            if let Some(target_ip) = crate::drivers::net::parse_ipv4(clean_target) {
                term_println(term, &format!("PING {}.{}.{}.{} 56(84) bytes of data.", target_ip[0], target_ip[1], target_ip[2], target_ip[3]));
                match crate::drivers::net::ping(target_ip) {
                    Ok(rtt_ms) => {
                        term_println(term, &format!("64 bytes from {}.{}.{}.{}: icmp_seq=1 ttl=64 time={} ms (Sugoi!)", target_ip[0], target_ip[1], target_ip[2], target_ip[3], rtt_ms));
                        vars.insert(String::from("?"), String::from("0"));
                    }
                    Err(e) => {
                        term_println(term, &format!("ping: request timeout for icmp_seq 1 ({}). Gomen nasai!", e));
                        vars.insert(String::from("?"), String::from("1"));
                    }
                }
            } else {
                term_println(term, &format!("ping: {}: Invalid network host address, b-baka!", clean_target));
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "ifconfig" => {
            if let Some(cfg) = crate::drivers::net::get_interface_config() {
                let m = cfg.mac;
                let ip = cfg.ip;
                let nm = cfg.netmask;
                let bc = cfg.broadcast;
                term_println(term, "eth0: flags=UP,BROADCAST,RUNNING,MULTICAST  mtu 1500");
                term_println(term, &format!("      inet {}.{}.{}.{}  netmask {}.{}.{}.{}  broadcast {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3], nm[0], nm[1], nm[2], nm[3], bc[0], bc[1], bc[2], bc[3]));
                term_println(term, &format!("      ether {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}  io 0x{:04X}", m[0], m[1], m[2], m[3], m[4], m[5], cfg.io_base));
                vars.insert(String::from("?"), String::from("0"));
            } else {
                term_println(term, "ifconfig: no network interface controller found (ENODEV). Doko desu ka?!");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "env" => {
            let mut buf = String::new();
            for (k, v) in vars.iter() {
                term_println(term, &format!("{}={}", k, v));
                buf.push_str(&format!("{}={}\n", k, v));
            }
            vars.insert(String::from("STDOUT"), buf);
            vars.insert(String::from("?"), String::from("0"));
        }
        "export" => {
            if args.is_empty() {
                for (k, v) in vars.iter() {
                    term_println(term, &format!("export {}={}", k, v));
                }
            } else {
                for arg in args {
                    if let Some((k, v)) = parse_assignment(arg) {
                        let eval_res = evaluate_expr(v, vars);
                        vars.insert(String::from(k), eval_res);
                    }
                }
            }
            vars.insert(String::from("?"), String::from("0"));
        }
        "unset" => {
            for arg in args {
                vars.remove(strip_quotes(arg));
            }
            vars.insert(String::from("?"), String::from("0"));
        }
        "test" | "[" => {
            let clean_args: Vec<&str> = if cmd_lower == "[" {
                if let Some(&"]") = args.last() {
                    args[..args.len() - 1].to_vec()
                } else {
                    term_println(term, "[: missing closing ']'");
                    vars.insert(String::from("?"), String::from("2"));
                    return;
                }
            } else {
                args.clone()
            };

            let cond_str = clean_args.join(" ");
            let ok = evaluate_condition(&cond_str, vars);
            vars.insert(String::from("?"), String::from(if ok { "0" } else { "1" }));
        }
        "echo" => {
            let raw_text = args.join(" ");
            let text = strip_quotes(&raw_text);
            term_println(term, text);
            vars.insert(String::from("STDOUT"), String::from(text));
            vars.insert(String::from("?"), String::from("0"));
        }
        "rm" => {
            let mut file_names: Vec<&str> = Vec::new();
            for a in args.iter() {
                if !a.starts_with('-') {
                    file_names.push(strip_quotes(*a));
                }
            }

            if let Some(&name) = file_names.get(0) {
                let start_ino = if name.starts_with('/') { 0 } else { *cwd_inode };
                let fs = VFS.lock();
                if fs.unlink(start_ino, name).is_ok() {
                    vars.insert(String::from("?"), String::from("0"));
                } else {
                    term_println(term, &format!("rm: cannot remove '{}': No such file or directory (ENOENT). Uso da!", name));
                    vars.insert(String::from("?"), String::from("1"));
                }
            } else {
                term_println(term, "rm: missing file operand, b-baka!");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "sh" => {
            if let Some(&script_name) = args.get(0) {
                let clean_name = strip_quotes(script_name);
                run_script_file(clean_name, cwd_inode, cwd_path, vars, aliases, history, term);
            } else {
                term_println(term, "sh: usage: sh <script.ks>");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "chmod" => {
            if args.len() >= 2 {
                let mode_str = strip_quotes(args[0]);
                let file_path = strip_quotes(args[1]);
                if let Ok(mode) = u16::from_str_radix(mode_str, 8) {
                    let start_ino = if file_path.starts_with('/') { 0 } else { *cwd_inode };
                    let fs = VFS.lock();
                    match fs.resolve_path(start_ino, file_path) {
                        Ok(ino) => {
                            if fs.chmod(ino, mode).is_ok() {
                                vars.insert(String::from("?"), String::from("0"));
                            } else {
                                term_println(term, &format!("chmod: cannot modify '{}': Operation not permitted (EPERM)", file_path));
                                vars.insert(String::from("?"), String::from("1"));
                            }
                        }
                        Err(_) => {
                            term_println(term, &format!("chmod: cannot access '{}': No such file or directory (ENOENT)", file_path));
                            vars.insert(String::from("?"), String::from("1"));
                        }
                    }
                } else {
                    term_println(term, &format!("chmod: invalid mode: '{}'", mode_str));
                    vars.insert(String::from("?"), String::from("1"));
                }
            } else {
                term_println(term, "chmod: missing operand");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "exit" => {
            let code = args.get(0).and_then(|c| strip_quotes(c).parse::<i32>().ok()).unwrap_or(0);
            if is_root {
                term_println(term, "[pam_auth]: root session closed, dropping privileges. Sayonara!");
                IS_ROOT.store(false, Ordering::Relaxed);
                vars.insert(String::from("USER"), String::from("senpai"));
                vars.insert(String::from("UID"), String::from("1000"));
                vars.insert(String::from("?"), format!("{}", code));
            } else {
                term_println(term, &format!("exit: Ja ne, Senpai! Session ended with status {}.", code));
                vars.insert(String::from("?"), format!("{}", code));
            }
        }
        "gui" | "desktop" => {
            crate::drivers::gfx::open_desktop();
            loop {
                if let Some(app) = crate::drivers::mouse::PENDING_APP_LAUNCH.lock().take() {
                    let clean_app = strip_quotes(&app);
                    let start_ino = if clean_app.starts_with('/') { 0 } else { *cwd_inode };
                    let _ = crate::usr::bin::execute_binary(start_ino, clean_app, &[]);
                    break;
                }
                if !crate::drivers::gfx::IS_DESKTOP_VIEW.load(Ordering::Relaxed) {
                    break;
                }
                x86_64::instructions::hlt();
            }
            vars.insert(String::from("?"), String::from("0"));
        }
        "pwd" => {
            term_println(term, cwd_path);
            vars.insert(String::from("STDOUT"), cwd_path.clone());
            vars.insert(String::from("?"), String::from("0"));
        }
        "cd" => {
            let target_raw = args.get(0).copied().unwrap_or_else(|| {
                vars.get("HOME").map(|s| s.as_str()).unwrap_or("/home")
            });
            let target = strip_quotes(target_raw);
            let start_ino = if target.starts_with('/') { 0 } else { *cwd_inode };
            let fs = VFS.lock();
            match fs.resolve_path(start_ino, target) {
                Ok(ino) => {
                    *cwd_inode = ino;
                    *cwd_path = canonicalize_path(cwd_path, target);
                    vars.insert(String::from("PWD"), cwd_path.clone());
                    vars.insert(String::from("?"), String::from("0"));
                }
                Err(e) => {
                    term_println(term, &format!("cd: {}: {} (Doko desu ka?!)", target, e.as_str()));
                    vars.insert(String::from("?"), String::from("1"));
                }
            }
        }
        "ls" => {
            let mut show_all = false;
            let mut long_format = false;
            let mut target = ".";

            for a in args.iter() {
                let clean_a = strip_quotes(a);
                if clean_a.starts_with('-') {
                    if clean_a.contains('a') { show_all = true; }
                    if clean_a.contains('l') { long_format = true; }
                } else {
                    target = clean_a;
                }
            }

            let start_ino = if target.starts_with('/') { 0 } else { *cwd_inode };
            let fs = VFS.lock();
            if let Ok(ino) = fs.resolve_path(start_ino, target) {
                if let Ok(entries) = fs.read_dir(ino) {
                    let mut stdout_buf = String::new();
                    if long_format {
                        term_println(term, &format!("total {}", entries.len()));
                    }
                    for (i, name, itype) in entries {
                        if !show_all && name.starts_with('.') {
                            continue;
                        }
                        if long_format {
                            let c = match itype {
                                InodeType::Directory => 'd',
                                InodeType::CharDev => 'c',
                                InodeType::BlockDev => 'b',
                                _ => '-',
                            };
                            let perm_str = if let Ok(inod) = fs.read_inode_public(i as usize) {
                                format_permissions(c, inod.permissions)
                            } else {
                                String::from("rwxr-xr-x")
                            };
                            term_println(term, &format!("{} 1 senpai senpai {:>4} {}", perm_str, i, name));
                            stdout_buf.push_str(&name);
                            stdout_buf.push('\n');
                        } else {
                            term_print(term, &format!("{}  ", name));
                            stdout_buf.push_str(&name);
                            stdout_buf.push(' ');
                        }
                    }
                    if !long_format {
                        term_println(term, "");
                    }
                    vars.insert(String::from("STDOUT"), stdout_buf);
                    vars.insert(String::from("?"), String::from("0"));
                } else if let Ok(inod) = fs.read_inode_public(ino) {
                    let perm_str = format_permissions('-', inod.permissions);
                    if long_format {
                        term_println(term, &format!("{} 1 senpai senpai {:>4} {}", perm_str, ino, target));
                    } else {
                        term_println(term, target);
                    }
                    vars.insert(String::from("STDOUT"), String::from(target));
                    vars.insert(String::from("?"), String::from("0"));
                } else {
                    term_println(term, &format!("ls: cannot access '{}': No such file or directory (ENOENT). Baka!", target));
                    vars.insert(String::from("?"), String::from("1"));
                }
            } else {
                term_println(term, &format!("ls: cannot access '{}': No such file or directory (ENOENT). Baka!", target));
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "cat" => {
            if let Some(&name) = args.get(0) {
                let clean_name = strip_quotes(name);
                let start_ino = if clean_name.starts_with('/') { 0 } else { *cwd_inode };
                let fs = VFS.lock();
                if let Ok(ino) = fs.resolve_path(start_ino, clean_name) {
                    if let Ok(d) = fs.read_inode_data(ino) {
                        if let Ok(s) = core::str::from_utf8(&d) {
                            term_print(term, s);
                            vars.insert(String::from("STDOUT"), String::from(s));
                        }
                        vars.insert(String::from("?"), String::from("0"));
                    } else {
                        term_println(term, &format!("cat: {}: Permission denied (EACCES)", clean_name));
                        vars.insert(String::from("?"), String::from("1"));
                    }
                } else {
                    term_println(term, &format!("cat: {}: No such file or directory (ENOENT). Nani?!", clean_name));
                    vars.insert(String::from("?"), String::from("1"));
                }
            } else {
                term_println(term, "cat: missing file operand, baka!");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "touch" => {
            if let Some(&name) = args.get(0) {
                let clean_name = strip_quotes(name);
                let _ = VFS.lock().create_file(*cwd_inode, clean_name, cur_uid, cur_gid);
                vars.insert(String::from("?"), String::from("0"));
            } else {
                term_println(term, "touch: missing file operand, b-baka!");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "mkdir" => {
            let mut target_name = None;
            for a in args.iter() {
                let clean_a = strip_quotes(a);
                if !clean_a.starts_with('-') {
                    target_name = Some(clean_a);
                    break;
                }
            }

            if let Some(name) = target_name {
                let _ = VFS.lock().create_dir(*cwd_inode, name, cur_uid, cur_gid);
                vars.insert(String::from("?"), String::from("0"));
            } else {
                term_println(term, "mkdir: missing directory operand");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "write" => {
            if args.len() >= 2 {
                let name = strip_quotes(args[0]);
                let raw_text = args[1..].join(" ");
                let text = strip_quotes(&raw_text);
                let fs = VFS.lock();
                let ino_res = fs.resolve_path(*cwd_inode, name).or_else(|_| fs.create_file(*cwd_inode, name, cur_uid, cur_gid));
                match ino_res {
                    Ok(ino) => {
                        let mut line_bytes = Vec::from(text.as_bytes());
                        line_bytes.push(b'\n');
                        let _ = fs.write_inode_data(ino, &line_bytes);
                        vars.insert(String::from("?"), String::from("0"));
                    }
                    Err(e) => {
                        term_println(term, &format!("write: {}: {}", name, e.as_str()));
                        vars.insert(String::from("?"), String::from("1"));
                    }
                }
            } else {
                term_println(term, "write: missing arguments");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "append" => {
            if args.len() >= 2 {
                let name = strip_quotes(args[0]);
                let raw_text = args[1..].join(" ");
                let text = strip_quotes(&raw_text);
                let fs = VFS.lock();
                let ino_res = fs.resolve_path(*cwd_inode, name).or_else(|_| fs.create_file(*cwd_inode, name, cur_uid, cur_gid));
                match ino_res {
                    Ok(ino) => {
                        let mut line_bytes = Vec::from(text.as_bytes());
                        line_bytes.push(b'\n');
                        let _ = fs.append_inode_data(ino, &line_bytes);
                        vars.insert(String::from("?"), String::from("0"));
                    }
                    Err(e) => {
                        term_println(term, &format!("append: {}: {}", name, e.as_str()));
                        vars.insert(String::from("?"), String::from("1"));
                    }
                }
            } else {
                term_println(term, "append: missing arguments");
                vars.insert(String::from("?"), String::from("1"));
            }
        }
        "clear" => {
            term.write_str("\x1b[2J");
            vars.insert(String::from("?"), String::from("0"));
        }
        "reboot" => {
            term_println(term, "Iku yo! Rebooting the system!");
            unsafe {
                while (crate::arch::x86_64::io::inb(0x64) & 2) != 0 {}
                crate::arch::x86_64::io::outb(0x64, 0xFE);
            }
            crate::arch::x86_64::hlt_loop();
        }
        prog => {
            let clean_prog = strip_quotes(prog);
            let start_ino = if clean_prog.starts_with('/') { 0 } else { *cwd_inode };
            let fs = VFS.lock();
            let exists = fs.resolve_path(start_ino, clean_prog).is_ok()
                || (!clean_prog.starts_with('/') && fs.resolve_path(0, &format!("/bin/{}", clean_prog)).is_ok());
            drop(fs);

            if exists {
                let ok = crate::usr::bin::execute_binary(start_ino, clean_prog, &args);
                vars.insert(String::from("?"), String::from(if ok { "0" } else { "1" }));
            } else {
                term_println(term, &format!("sh: {}: command not found (ENOEXEC). Anta baka?!", clean_prog));
                vars.insert(String::from("?"), String::from("127"));
            }
        }
    }
}

fn execute_pipeline_left(
    cmd_line: &str,
    cwd_inode: &mut usize,
    cwd_path: &mut String,
    vars: &mut BTreeMap<String, String>,
    aliases: &mut BTreeMap<String, String>,
    history: &mut Vec<String>,
    term: &mut crate::drivers::wm::TerminalDescriptor,
) -> String {
    let expanded = expand_vars(cmd_line, vars);
    let mut words = split_words(&expanded);
    if words.is_empty() {
        return String::new();
    }
    if let Some(alias_val) = aliases.get(&words[0]) {
        let mut exp_alias = alias_val.clone();
        for arg in &words[1..] {
            exp_alias.push(' ');
            exp_alias.push_str(arg);
        }
        words = split_words(&exp_alias);
        if words.is_empty() { return String::new(); }
    }
    let raw_cmd = &words[0];
    let args: Vec<&str> = words[1..].iter().map(|s| s.as_str()).collect();
    let mut cmd_lower = String::with_capacity(raw_cmd.len());
    for c in raw_cmd.chars() {
        cmd_lower.push(c.to_ascii_lowercase());
    }

    match cmd_lower.as_str() {
        "echo" => {
            let raw_text = args.join(" ");
            String::from(strip_quotes(&raw_text))
        }
        "pwd" => cwd_path.clone(),
        "whoami" => vars.get("USER").cloned().unwrap_or_else(|| String::from("unknown")),
        "id" => {
            if IS_ROOT.load(Ordering::Relaxed) {
                String::from("uid=0(root) gid=0(root) groups=0(root)")
            } else {
                String::from("uid=1000(senpai) gid=1000(senpai) groups=1000(senpai)")
            }
        }
        "uname" => {
            if args.iter().any(|&a| a == "-a") {
                String::from("redKiteOS localhost 4.0-zsh redKiteOS-posix x86_64")
            } else {
                String::from("redKiteOS")
            }
        }
        "cat" => {
            if let Some(&name) = args.get(0) {
                let clean_name = strip_quotes(name);
                let start_ino = if clean_name.starts_with('/') { 0 } else { *cwd_inode };
                let fs = VFS.lock();
                if let Ok(ino) = fs.resolve_path(start_ino, clean_name) {
                    if let Ok(d) = fs.read_inode_data(ino) {
                        String::from_utf8(d).unwrap_or_else(|_| String::new())
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        }
        "ls" => {
            let target_raw = args.get(0).copied().unwrap_or(".");
            let target = strip_quotes(target_raw);
            let start_ino = if target.starts_with('/') { 0 } else { *cwd_inode };
            let fs = VFS.lock();
            if let Ok(ino) = fs.resolve_path(start_ino, target) {
                if let Ok(entries) = fs.read_dir(ino) {
                    let mut s = String::new();
                    for (_, name, _) in entries {
                        if !name.starts_with('.') {
                            if !s.is_empty() { s.push(' '); }
                            s.push_str(&name);
                        }
                    }
                    s
                } else if fs.read_inode_public(ino).is_ok() {
                    String::from(target)
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        }
        _ => {
            execute_script_line(cmd_line, cwd_inode, cwd_path, vars, aliases, history, term);
            vars.get("STDOUT").cloned().unwrap_or_else(String::new)
        }
    }
}

fn format_permissions(type_char: char, mode: u16) -> String {
    let mut s = String::with_capacity(10);
    s.push(type_char);
    let rwx = [
        (0o400, 'r'), (0o200, 'w'), (0o100, 'x'),
        (0o040, 'r'), (0o020, 'w'), (0o010, 'x'),
        (0o004, 'r'), (0o002, 'w'), (0o001, 'x'),
    ];
    for &(mask, ch) in &rwx {
        if (mode & mask) != 0 {
            s.push(ch);
        } else {
            s.push('-');
        }
    }
    s
}

fn canonicalize_path(current: &str, target: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if !target.starts_with('/') {
        for p in current.split('/') {
            if !p.is_empty() {
                parts.push(p);
            }
        }
    }

    for p in target.split('/') {
        if p.is_empty() || p == "." {
            continue;
        } else if p == ".." {
            parts.pop();
        } else {
            parts.push(p);
        }
    }

    if parts.is_empty() {
        String::from("/")
    } else {
        let mut res = String::new();
        for p in parts {
            res.push('/');
            res.push_str(p);
        }
        res
    }
}

fn run_script_file(
    script_name: &str,
    cwd_inode: &mut usize,
    cwd_path: &mut String,
    vars: &mut BTreeMap<String, String>,
    aliases: &mut BTreeMap<String, String>,
    history: &mut Vec<String>,
    term: &mut crate::drivers::wm::TerminalDescriptor,
) {
    let clean_name = strip_quotes(script_name);
    let start_ino = if clean_name.starts_with('/') { 0 } else { *cwd_inode };
    let content = {
        let fs = VFS.lock();
        if let Ok(ino) = fs.resolve_path(start_ino, clean_name) {
            if let Ok(bytes) = fs.read_inode_data(ino) {
                String::from_utf8(bytes).ok()
            } else {
                None
            }
        } else {
            None
        }
    };

    match content {
        Some(script) => {
            for raw_line in script.lines() {
                execute_script_line(raw_line, cwd_inode, cwd_path, vars, aliases, history, term);
            }
            vars.insert(String::from("?"), String::from("0"));
        }
        None => {
            term_println(term, &format!("sh: {}: No such file or directory (ENOENT). Baka!", clean_name));
            vars.insert(String::from("?"), String::from("127"));
        }
    }
}

fn expand_vars(line: &str, vars: &BTreeMap<String, String>) -> String {
    let mut res = String::new();
    let mut chars = line.chars().peekable();
    let mut in_single_quote = false;

    while let Some(c) = chars.next() {
        if c == '\'' {
            in_single_quote = !in_single_quote;
            res.push(c);
        } else if c == '$' && !in_single_quote {
            let mut var_name = String::new();
            if let Some(&special) = chars.peek() {
                if special == '?' || special == '$' || special == '0' || special == '!' {
                    var_name.push(special);
                    chars.next();
                    if let Some(val) = vars.get(&var_name) {
                        res.push_str(val);
                    }
                    continue;
                }
            }
            while let Some(&next_c) = chars.peek() {
                if next_c.is_alphanumeric() || next_c == '_' || next_c == '.' || next_c == '[' || next_c == ']' {
                    var_name.push(next_c);
                    chars.next();
                } else {
                    break;
                }
            }
            if !var_name.is_empty() {
                if let Some(val) = vars.get(&var_name) {
                    res.push_str(val);
                }
            }
        } else {
            res.push(c);
        }
    }
    res
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
}

fn tokenize(expr: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '"' | '\'' => { chars.next(); }
            '+' => { tokens.push(Token::Plus); chars.next(); }
            '-' => { tokens.push(Token::Minus); chars.next(); }
            '*' => { tokens.push(Token::Star); chars.next(); }
            '/' => { tokens.push(Token::Slash); chars.next(); }
            '%' => { tokens.push(Token::Percent); chars.next(); }
            '(' => { tokens.push(Token::LParen); chars.next(); }
            ')' => { tokens.push(Token::RParen); chars.next(); }
            '0'..='9' => {
                let mut num = 0i64;
                while let Some(&d) = chars.peek() {
                    if let Some(digit) = d.to_digit(10) {
                        num = num.saturating_mul(10).saturating_add(digit as i64);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Number(num));
            }
            'a'..='z' | 'A'..='Z' | '_' | '$' => {
                let mut s = String::new();
                if c == '$' { chars.next(); }
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '[' || ch == ']' {
                        s.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !s.is_empty() {
                    tokens.push(Token::Ident(s));
                }
            }
            _ => { chars.next(); }
        }
    }
    tokens
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    vars: &'a BTreeMap<String, String>,
    has_div_zero: bool,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], vars: &'a BTreeMap<String, String>) -> Self {
        Parser { tokens, pos: 0, vars, has_div_zero: false }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next_token(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    fn parse_expr(&mut self) -> i64 {
        let mut left = self.parse_term();
        while let Some(t) = self.peek() {
            match t {
                Token::Plus => { self.next_token(); left = left.saturating_add(self.parse_term()); }
                Token::Minus => { self.next_token(); left = left.saturating_sub(self.parse_term()); }
                _ => break,
            }
        }
        left
    }

    fn parse_term(&mut self) -> i64 {
        let mut left = self.parse_factor();
        while let Some(t) = self.peek() {
            match t {
                Token::Star => { self.next_token(); left = left.saturating_mul(self.parse_factor()); }
                Token::Slash => {
                    self.next_token();
                    let right = self.parse_factor();
                    if right == 0 {
                        self.has_div_zero = true;
                        left = 0;
                    } else {
                        left = left / right;
                    }
                }
                Token::Percent => {
                    self.next_token();
                    let right = self.parse_factor();
                    if right == 0 {
                        self.has_div_zero = true;
                        left = 0;
                    } else {
                        left = left % right;
                    }
                }
                _ => break,
            }
        }
        left
    }

    fn parse_factor(&mut self) -> i64 {
        match self.next_token() {
            Some(Token::Number(n)) => n,
            Some(Token::Ident(id)) => {
                if let Some(val_str) = self.vars.get(&id) {
                    val_str.trim().parse::<i64>().unwrap_or(0)
                } else {
                    0
                }
            }
            Some(Token::LParen) => {
                let val = self.parse_expr();
                if let Some(Token::RParen) = self.peek() {
                    self.next_token();
                }
                val
            }
            Some(Token::Minus) => -self.parse_factor(),
            _ => 0,
        }
    }
}

fn evaluate_expr(expr: &str, vars: &BTreeMap<String, String>) -> String {
    let clean = strip_quotes(expr);
    let tokens = tokenize(clean);
    let mut parser = Parser::new(&tokens, vars);
    let result = parser.parse_expr();

    if parser.has_div_zero {
        return String::from("0");
    }

    if tokens.iter().any(|t| matches!(t, Token::Plus | Token::Minus | Token::Star | Token::Slash | Token::Percent)) {
        return format!("{}", result);
    }

    if let Some(Token::Number(n)) = tokens.first() {
        if tokens.len() == 1 {
            return format!("{}", n);
        }
    }

    if let Some(Token::Ident(id)) = tokens.first() {
        if tokens.len() == 1 {
            if let Some(val) = vars.get(id) {
                return val.clone();
            }
            return id.clone();
        }
    }

    expand_vars(clean, vars)
}

fn evaluate_condition(cond: &str, vars: &BTreeMap<String, String>) -> bool {
    let ops = ["==", "!=", "<=", ">=", "<", ">", "-eq", "-ne", "-lt", "-gt", "-le", "-ge"];
    for op in ops {
        if let Some(pos) = cond.find(op) {
            let left_str = cond[..pos].trim();
            let right_str = cond[pos + op.len()..].trim();

            let left_val = evaluate_expr(left_str, vars).parse::<i64>();
            let right_val = evaluate_expr(right_str, vars).parse::<i64>();

            if let (Ok(l), Ok(r)) = (left_val, right_val) {
                return match op {
                    "==" | "-eq" => l == r,
                    "!=" | "-ne" => l != r,
                    "<" | "-lt" => l < r,
                    ">" | "-gt" => l > r,
                    "<=" | "-le" => l <= r,
                    ">=" | "-ge" => l >= r,
                    _ => false,
                };
            } else {
                let l_s = expand_vars(left_str, vars);
                let r_s = expand_vars(right_str, vars);
                return match op {
                    "==" | "-eq" => l_s == r_s,
                    "!=" | "-ne" => l_s != r_s,
                    _ => false,
                };
            }
        }
    }
    false
}

fn execute_if_statement(
    line: &str,
    cwd_inode: &mut usize,
    cwd_path: &mut String,
    vars: &mut BTreeMap<String, String>,
    aliases: &mut BTreeMap<String, String>,
    history: &mut Vec<String>,
    term: &mut crate::drivers::wm::TerminalDescriptor,
) {
    let then_pos = match line.find(" then ") {
        Some(p) => p,
        None => return,
    };
    let cond = line[3..then_pos].trim();
    let body = line[then_pos + 6..].trim();
    let body_clean = body.trim_end_matches(" fi").trim();

    if evaluate_condition(cond, vars) {
        for sub_cmd in body_clean.split(';') {
            execute_script_line(sub_cmd.trim(), cwd_inode, cwd_path, vars, aliases, history, term);
        }
    }
}

fn execute_while_loop(
    line: &str,
    cwd_inode: &mut usize,
    cwd_path: &mut String,
    vars: &mut BTreeMap<String, String>,
    aliases: &mut BTreeMap<String, String>,
    history: &mut Vec<String>,
    term: &mut crate::drivers::wm::TerminalDescriptor,
) {
    let do_pos = match line.find(" do ") {
        Some(p) => p,
        None => return,
    };
    let cond = line[6..do_pos].trim();
    let body = line[do_pos + 4..].trim();
    let body_clean = body.trim_end_matches(" done").trim();

    let mut iterations = 0;
    while evaluate_condition(cond, vars) && iterations < 1000 {
        iterations += 1;
        for sub_cmd in body_clean.split(';') {
            execute_script_line(sub_cmd.trim(), cwd_inode, cwd_path, vars, aliases, history, term);
        }
    }
}

fn execute_for_loop(
    line: &str,
    cwd_inode: &mut usize,
    cwd_path: &mut String,
    vars: &mut BTreeMap<String, String>,
    aliases: &mut BTreeMap<String, String>,
    history: &mut Vec<String>,
    term: &mut crate::drivers::wm::TerminalDescriptor,
) {
    let in_pos = match line.find(" in ") {
        Some(p) => p,
        None => return,
    };
    let do_pos = match line.find(" do ") {
        Some(p) => p,
        None => return,
    };
    if in_pos >= do_pos { return; }

    let var_name = line[4..in_pos].trim();
    let items_str = line[in_pos + 4..do_pos].trim();
    let body = line[do_pos + 4..].trim();
    let body_clean = body.trim_end_matches(" done").trim();

    for item in items_str.split_whitespace() {
        vars.insert(String::from(var_name), String::from(strip_quotes(item)));
        for sub_cmd in body_clean.split(';') {
            execute_script_line(sub_cmd.trim(), cwd_inode, cwd_path, vars, aliases, history, term);
        }
    }
}

fn clean_string(s: &str) -> &str {
    let trimmed = s.trim_matches(|c: char| c.is_whitespace() || c.is_control());
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        trimmed[1..trimmed.len() - 1].trim()
    } else {
        trimmed
    }
}