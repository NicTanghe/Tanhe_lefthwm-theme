use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SEP: char = '\x1f';

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Number(f64),
    Ident,
    Plus,
    Minus,
    Star,
    Slash,
    DoubleSlash,
    Percent,
    Caret,
    DoubleStar,
    LParen,
    RParen,
    Comma,
    End,
}

#[derive(Debug, Clone)]
struct LexToken {
    kind: Token,
    text: String,
}

struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn next_token(&mut self) -> Result<LexToken, String> {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }

        if self.pos >= self.bytes.len() {
            return Ok(LexToken {
                kind: Token::End,
                text: String::new(),
            });
        }

        let start = self.pos;
        let byte = self.bytes[self.pos];

        if byte.is_ascii_digit() || byte == b'.' {
            self.read_number(start)
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            self.pos += 1;
            while self.pos < self.bytes.len()
                && (self.bytes[self.pos].is_ascii_alphanumeric() || self.bytes[self.pos] == b'_')
            {
                self.pos += 1;
            }
            Ok(LexToken {
                kind: Token::Ident,
                text: self.input[start..self.pos].to_string(),
            })
        } else {
            self.pos += 1;
            let kind = match byte {
                b'+' => Token::Plus,
                b'-' => Token::Minus,
                b'*' if self.peek_byte() == Some(b'*') => {
                    self.pos += 1;
                    Token::DoubleStar
                }
                b'*' => Token::Star,
                b'/' if self.peek_byte() == Some(b'/') => {
                    self.pos += 1;
                    Token::DoubleSlash
                }
                b'/' => Token::Slash,
                b'%' => Token::Percent,
                b'^' => Token::Caret,
                b'(' => Token::LParen,
                b')' => Token::RParen,
                b',' => Token::Comma,
                _ => return Err("unsupported character".to_string()),
            };
            Ok(LexToken {
                kind,
                text: self.input[start..self.pos].to_string(),
            })
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn read_number(&mut self, start: usize) -> Result<LexToken, String> {
        let mut saw_digit = false;

        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            saw_digit = true;
            self.pos += 1;
        }

        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'.' {
            self.pos += 1;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                saw_digit = true;
                self.pos += 1;
            }
        }

        if !saw_digit {
            return Err("number expected".to_string());
        }

        if self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b'e' | b'E') {
            let exponent_pos = self.pos;
            self.pos += 1;
            if self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b'+' | b'-') {
                self.pos += 1;
            }
            let exponent_start = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if exponent_start == self.pos {
                self.pos = exponent_pos;
            }
        }

        let text = &self.input[start..self.pos];
        let value = text
            .parse::<f64>()
            .map_err(|_| "invalid number".to_string())?;
        Ok(LexToken {
            kind: Token::Number(value),
            text: text.to_string(),
        })
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    current: LexToken,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Result<Self, String> {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token()?;
        Ok(Self { lexer, current })
    }

    fn advance(&mut self) -> Result<(), String> {
        self.current = self.lexer.next_token()?;
        Ok(())
    }

    fn parse(mut self) -> Result<f64, String> {
        let value = self.parse_add_sub()?;
        if self.current.kind != Token::End {
            return Err("unexpected input".to_string());
        }
        if !value.is_finite() {
            return Err("non-finite result".to_string());
        }
        Ok(value)
    }

    fn parse_add_sub(&mut self) -> Result<f64, String> {
        let mut value = self.parse_mul_div()?;
        loop {
            match self.current.kind {
                Token::Plus => {
                    self.advance()?;
                    value += self.parse_mul_div()?;
                }
                Token::Minus => {
                    self.advance()?;
                    value -= self.parse_mul_div()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_mul_div(&mut self) -> Result<f64, String> {
        let mut value = self.parse_unary()?;
        loop {
            match self.current.kind {
                Token::Star => {
                    self.advance()?;
                    value *= self.parse_unary()?;
                }
                Token::Slash => {
                    self.advance()?;
                    value /= self.parse_unary()?;
                }
                Token::DoubleSlash => {
                    self.advance()?;
                    value = (value / self.parse_unary()?).floor();
                }
                Token::Percent => {
                    self.advance()?;
                    value %= self.parse_unary()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_unary(&mut self) -> Result<f64, String> {
        match self.current.kind {
            Token::Plus => {
                self.advance()?;
                self.parse_unary()
            }
            Token::Minus => {
                self.advance()?;
                Ok(-self.parse_unary()?)
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<f64, String> {
        let left = self.parse_primary()?;
        match self.current.kind {
            Token::Caret | Token::DoubleStar => {
                self.advance()?;
                let right = self.parse_unary()?;
                if right.abs() > 1000.0 {
                    return Err("exponent too large".to_string());
                }
                Ok(left.powf(right))
            }
            _ => Ok(left),
        }
    }

    fn parse_primary(&mut self) -> Result<f64, String> {
        match self.current.kind {
            Token::Number(value) => {
                self.advance()?;
                Ok(value)
            }
            Token::Ident => {
                let name = self.current.text.to_ascii_lowercase();
                self.advance()?;
                if self.current.kind == Token::LParen {
                    self.parse_call(&name)
                } else {
                    constant(&name)
                }
            }
            Token::LParen => {
                self.advance()?;
                let value = self.parse_add_sub()?;
                if self.current.kind != Token::RParen {
                    return Err("missing ')'".to_string());
                }
                self.advance()?;
                Ok(value)
            }
            _ => Err("expression expected".to_string()),
        }
    }

    fn parse_call(&mut self, name: &str) -> Result<f64, String> {
        self.advance()?;
        let mut args = Vec::new();
        if self.current.kind != Token::RParen {
            loop {
                args.push(self.parse_add_sub()?);
                if self.current.kind == Token::Comma {
                    self.advance()?;
                } else {
                    break;
                }
            }
        }
        if self.current.kind != Token::RParen {
            return Err("missing ')'".to_string());
        }
        self.advance()?;
        function(name, &args)
    }
}

fn constant(name: &str) -> Result<f64, String> {
    match name {
        "e" => Ok(std::f64::consts::E),
        "pi" => Ok(std::f64::consts::PI),
        "tau" => Ok(std::f64::consts::TAU),
        _ => Err("unknown name".to_string()),
    }
}

fn function(name: &str, args: &[f64]) -> Result<f64, String> {
    let one = || {
        if args.len() == 1 {
            Ok(args[0])
        } else {
            Err("wrong number of arguments".to_string())
        }
    };
    let two = || {
        if args.len() == 2 {
            Ok((args[0], args[1]))
        } else {
            Err("wrong number of arguments".to_string())
        }
    };

    match name {
        "abs" => Ok(one()?.abs()),
        "acos" => Ok(one()?.acos()),
        "asin" => Ok(one()?.asin()),
        "atan" => Ok(one()?.atan()),
        "ceil" => Ok(one()?.ceil()),
        "cos" => Ok(one()?.cos()),
        "exp" => Ok(one()?.exp()),
        "floor" => Ok(one()?.floor()),
        "ln" | "log" => Ok(one()?.ln()),
        "log10" => Ok(one()?.log10()),
        "pow" => {
            let (base, exponent) = two()?;
            if exponent.abs() > 1000.0 {
                return Err("exponent too large".to_string());
            }
            Ok(base.powf(exponent))
        }
        "round" if args.len() == 1 => Ok(args[0].round()),
        "round" if args.len() == 2 => {
            let places = args[1].round().clamp(-12.0, 12.0);
            let scale = 10_f64.powf(places);
            Ok((args[0] * scale).round() / scale)
        }
        "sin" => Ok(one()?.sin()),
        "sqrt" => Ok(one()?.sqrt()),
        "tan" => Ok(one()?.tan()),
        "max" if !args.is_empty() => Ok(args.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        "min" if !args.is_empty() => Ok(args.iter().copied().fold(f64::INFINITY, f64::min)),
        "max" | "min" => Err("wrong number of arguments".to_string()),
        _ => Err("unknown function".to_string()),
    }
}

fn calculate(input: &str) -> Result<String, String> {
    let mut expression = input.trim();
    if expression.to_ascii_lowercase().starts_with("calc ") {
        expression = expression[5..].trim();
    }
    if expression.starts_with('=') {
        expression = expression[1..].trim();
    }
    if expression.is_empty() {
        return Err("type an expression".to_string());
    }
    if expression.len() > 200 {
        return Err("expression too long".to_string());
    }

    let value = Parser::new(expression)?.parse()?;
    Ok(format_number(value))
}

fn looks_like_plain_app_search(input: &str) -> bool {
    let query = input.trim();
    !query.starts_with('=')
        && !query.to_ascii_lowercase().starts_with("calc ")
        && query.chars().any(|ch| ch.is_ascii_alphabetic())
        && !query.contains('(')
        && !query.contains(')')
}

#[derive(Debug, Clone)]
struct DesktopApp {
    id: String,
    name: String,
    icon: String,
    meta: String,
    path: String,
}

fn desktop_value(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => output.push('\n'),
                Some('r') => output.push('\r'),
                Some('t') => output.push('\t'),
                Some('s') => output.push(' '),
                Some('\\') => output.push('\\'),
                Some(next) => output.push(next),
                None => output.push('\\'),
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn truthy(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("true")
}

fn desktop_id(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".desktop"))
        .map(|name| name.to_string())
}

fn parse_desktop_app(path: &Path) -> Option<DesktopApp> {
    let content = fs::read_to_string(path).ok()?;
    let id = desktop_id(path)?;

    let mut in_desktop_entry = false;
    let mut app_type = String::new();
    let mut name = String::new();
    let mut generic_name = String::new();
    let mut comment = String::new();
    let mut exec = String::new();
    let mut icon = String::new();
    let mut categories = String::new();
    let mut keywords = String::new();
    let mut no_display = false;
    let mut hidden = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = desktop_value(value.trim());

        match key {
            "Type" => app_type = value,
            "Name" => name = value,
            "GenericName" => generic_name = value,
            "Comment" => comment = value,
            "Exec" => exec = value,
            "Icon" => icon = value,
            "Categories" => categories = value,
            "Keywords" => keywords = value,
            "NoDisplay" => no_display = truthy(&value),
            "Hidden" => hidden = truthy(&value),
            _ => {}
        }
    }

    if app_type != "Application" || name.is_empty() || exec.is_empty() || no_display || hidden {
        return None;
    }

    let meta = [
        id.as_str(),
        name.as_str(),
        generic_name.as_str(),
        comment.as_str(),
        exec.as_str(),
        categories.as_str(),
        keywords.as_str(),
    ]
    .iter()
    .copied()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" ");

    Some(DesktopApp {
        id,
        name,
        icon,
        meta,
        path: path.to_string_lossy().to_string(),
    })
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    {
        dirs.push(home.join("applications"));
    }

    let xdg_data_dirs =
        env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    dirs.extend(
        xdg_data_dirs
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| PathBuf::from(dir).join("applications")),
    );

    dirs
}

fn state_dir() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|dir| dir.join("rofi-calc"))
}

fn app_usage_path() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("app-usage.tsv"))
}

fn load_app_usage() -> HashMap<String, u64> {
    let Some(path) = app_usage_path() else {
        return HashMap::new();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return HashMap::new();
    };

    content
        .lines()
        .filter_map(|line| {
            let (id, count) = line.split_once('\t')?;
            let count = count.parse::<u64>().ok()?;
            Some((id.to_string(), count))
        })
        .collect()
}

fn save_app_usage(usage: &HashMap<String, u64>) -> io::Result<()> {
    let Some(path) = app_usage_path() else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut entries = usage.iter().collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));

    let mut content = String::new();
    for (id, count) in entries {
        if *count == 0 {
            continue;
        }
        content.push_str(id);
        content.push('\t');
        content.push_str(&count.to_string());
        content.push('\n');
    }

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, content)?;
    fs::rename(temp_path, path)?;
    Ok(())
}

fn increment_app_usage(id: &str) {
    let mut usage = load_app_usage();
    let count = usage.entry(id.to_string()).or_insert(0);
    *count = (*count).saturating_add(1);
    let _ = save_app_usage(&usage);
}

fn desktop_apps() -> Vec<DesktopApp> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();

    for dir in data_dirs() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }

            let Some(id) = desktop_id(&path) else {
                continue;
            };

            if !seen.insert(id) {
                continue;
            }

            if let Some(app) = parse_desktop_app(&path) {
                apps.push(app);
            }
        }
    }

    let usage = load_app_usage();
    apps.sort_by(|left, right| {
        let left_count = usage.get(&left.id).copied().unwrap_or(0);
        let right_count = usage.get(&right.id).copied().unwrap_or(0);

        right_count
            .cmp(&left_count)
            .then_with(|| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()))
            .then_with(|| left.id.cmp(&right.id))
    });
    apps
}

fn launch_app(id: &str, path: &str) -> bool {
    if command_exists("gio")
        && Command::new("gio")
            .arg("launch")
            .arg(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    {
        return true;
    }

    if command_exists("gtk-launch") {
        if Command::new("gtk-launch")
            .arg(id)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return true;
        }

        let desktop_name = format!("{id}.desktop");
        if Command::new("gtk-launch")
            .arg(desktop_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return true;
        }
    }

    false
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() <= i64::MAX as f64 {
        return format!("{}", value as i64);
    }

    let abs = value.abs();
    let mut text = if abs != 0.0 && !(1e-6..1e12).contains(&abs) {
        format!("{value:.12e}")
    } else {
        format!("{value:.12}")
    };

    if let Some(exponent_start) = text.find('e') {
        let (mantissa, exponent) = text.split_at(exponent_start);
        let mut mantissa = mantissa.trim_end_matches('0').trim_end_matches('.').to_string();
        mantissa.push_str(exponent);
        text = mantissa;
    } else {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    text
}

fn clean(value: &str) -> String {
    value.replace(['\n', '\r', '\0'], " ")
}

fn option(key: &str, value: &str) {
    print!("\0{key}{SEP}{}\n", clean(value));
}

fn row(text: &str, fields: &[(&str, &str)]) {
    print!("{}", clean(text));
    if !fields.is_empty() {
        print!("\0");
        for (index, (key, value)) in fields.iter().enumerate() {
            if index > 0 {
                print!("{SEP}");
            }
            print!("{key}{SEP}{}", clean(value));
        }
    }
    println!();
}

fn app_matches_query(app: &DesktopApp, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }

    let searchable = format!("{} {}", app.name, app.meta).to_ascii_lowercase();
    query
        .to_ascii_lowercase()
        .split_whitespace()
        .all(|token| searchable.contains(token))
}

fn render(result: Option<&str>, _message: &str, _copied: bool, _show_hint: bool, query: &str) {
    const MAX_COLUMNS: usize = 8;

    let apps: Vec<_> = desktop_apps()
        .into_iter()
        .filter(|app| app_matches_query(app, query))
        .collect();
    let hit_count = apps.len() + usize::from(result.is_some());
    let active_columns = if hit_count > 0 && hit_count % 2 == 1 {
        MAX_COLUMNS - 1
    } else {
        MAX_COLUMNS
    };
    let remainder = hit_count % active_columns;
    let partial_row_start = hit_count.saturating_sub(remainder);
    let left_spacers = if remainder == 0 {
        0
    } else {
        (active_columns - remainder) / 2
    };
    let right_spacers = if remainder == 0 {
        0
    } else {
        active_columns - remainder - left_spacers
    };
    let mut emitted_items = 0usize;
    let mut spacer_id = 0usize;
    let list_width = 61.8034 * active_columns as f64 / MAX_COLUMNS as f64;
    let list_margin = (100.0 - list_width) / 2.0;

    option("prompt", "Run");
    option("no-custom", "false");
    option("use-hot-keys", "true");
    option("keep-filter", "true");
    option("keep-selection", "true");
    let theme = format!(
        "listview {{ columns: {active_columns}; width: {list_width:.4}%; margin: 48px {list_margin:.4}% 48px; }}"
    );
    option("theme", &theme);
    let new_selection = if hit_count > 0 && partial_row_start == 0 && remainder != 0 {
        left_spacers
    } else {
        0
    };
    option("new-selection", &new_selection.to_string());
    option("data", result.unwrap_or(""));

    let emit_spacer = |spacer_id: &mut usize| {
        let row_id = format!("spacer:{spacer_id}");
        *spacer_id += 1;
        row(
            &row_id,
            &[
                ("display", " "),
                ("permanent", "true"),
                ("nonselectable", "true"),
            ],
        );
    };

    let emit_left_spacers = |emitted_items: usize, spacer_id: &mut usize| {
        if remainder != 0 && emitted_items == partial_row_start {
            for _ in 0..left_spacers {
                emit_spacer(spacer_id);
            }
        }
    };

    if hit_count == 0 {
        emit_spacer(&mut spacer_id);
        return;
    }

    if let Some(value) = result {
        emit_left_spacers(emitted_items, &mut spacer_id);
        let info = format!("copy:{value}");
        let display = format!("Copy  {value}");
        row(
            "copy-result",
            &[
                ("display", &display),
                ("icon", "edit-copy"),
                ("info", &info),
                ("permanent", "true"),
                ("active", "true"),
            ],
        );
        emitted_items += 1;
    }

    for app in apps {
        emit_left_spacers(emitted_items, &mut spacer_id);
        let info = format!("app:{}\t{}", app.id, app.path);
        let row_id = format!("app:{}", app.id);
        row(
            &row_id,
            &[
                ("display", app.name.as_str()),
                ("icon", app.icon.as_str()),
                ("meta", app.meta.as_str()),
                ("info", &info),
            ],
        );
        emitted_items += 1;
    }

    if remainder != 0 {
        for _ in 0..right_spacers {
            emit_spacer(&mut spacer_id);
        }
    }
}

fn command_exists(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path).any(|dir| {
        let candidate: PathBuf = dir.join(name);
        fs::metadata(candidate)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

fn copy_to_clipboard(text: &str) -> io::Result<()> {
    let mut candidates: Vec<(&str, &[&str])> = Vec::new();
    if env::var_os("WAYLAND_DISPLAY").is_some() && command_exists("wl-copy") {
        candidates.push(("wl-copy", &[]));
    }
    if env::var_os("DISPLAY").is_some() && command_exists("xsel") {
        candidates.push(("xsel", &["-ib"]));
    }
    if env::var_os("DISPLAY").is_some() && command_exists("xclip") {
        candidates.push(("xclip", &["-selection", "clipboard"]));
    }

    for (program, args) in candidates {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }

        if child.wait()?.success() {
            return Ok(());
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no clipboard helper available",
    ))
}

fn main() {
    let retv = env::var("ROFI_RETV").unwrap_or_else(|_| "0".to_string());
    let argument = env::args().nth(1).unwrap_or_default();
    let data = env::var("ROFI_DATA").unwrap_or_default();
    let info = env::var("ROFI_INFO").unwrap_or_default();

    if retv == "1" {
        if let Some(payload) = info.strip_prefix("app:") {
            if let Some((id, path)) = payload.split_once('\t') {
                if launch_app(id, path) {
                    increment_app_usage(id);
                }
                return;
            }
        }

        let result = if let Some(result) = info.trim().strip_prefix("copy:") {
            result.to_string()
        } else if !info.trim().is_empty() {
            info.trim().to_string()
        } else {
            data.trim().to_string()
        };

        if result.is_empty() {
            render(None, "No result to copy", false, true, "");
            return;
        }

        let copied = copy_to_clipboard(&result).is_ok();
        render(
            Some(&result),
            if copied { "Copied" } else { "Copy failed" },
            copied,
            true,
            "",
        );
        return;
    }

    if retv == "10" {
        let result = data.trim().to_string();

        if result.is_empty() {
            render(None, "No result to copy", false, true, "");
            return;
        }

        let copied = copy_to_clipboard(&result).is_ok();
        render(
            Some(&result),
            if copied { "Copied" } else { "Copy failed" },
            copied,
            true,
            "",
        );
        return;
    }

    if retv == "2" && !argument.trim().is_empty() {
        if looks_like_plain_app_search(&argument) {
            render(None, "Applications", false, false, &argument);
            return;
        }

        match calculate(&argument) {
            Ok(result) => render(Some(&result), "", false, true, &argument),
            Err(_) => render(None, "", false, true, &argument),
        }
        return;
    }

    render(None, "", false, true, "");
}
