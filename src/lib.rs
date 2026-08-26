use std::{
    collections::HashMap,
    env,
    fmt,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    Boolean(bool),
    List(Vec<Value>),
    Nothing,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(value) => write!(f, "{value}"),
            Self::Number(value) if value.fract() == 0.0 => write!(f, "{value:.0}"),
            Self::Number(value) => write!(f, "{value}"),
            Self::Boolean(value) => write!(f, "{value}"),
            Self::List(values) => {
                let values = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                write!(f, "[{values}]")
            }
            Self::Nothing => write!(f, "nothing"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Set { name: String, value: Expression },
    SetEnvironment { name: Expression, value: Expression },
    Say(Expression),
    Run { application: String, arguments: Vec<Expression> },
    CreateFolder(Expression),
    Copy { source: Expression, destination: Expression },
    Move { source: Expression, destination: Expression },
    DeleteFile(Expression),
    DeleteFolder(Expression),
    WriteFile { content: Expression, path: Expression },
    Shell(Expression),
    StartShell(Expression),
    OpenFile(Expression),
    Include(Expression),
    Stop,
    Skip,
    When { condition: Condition, body: Vec<Statement>, otherwise: Option<Vec<Statement>> },
    Repeat { count: Expression, body: Vec<Statement> },
    While { condition: Condition, body: Vec<Statement> },
    ForEach { name: String, iterable: Expression, body: Vec<Statement> },
    Define { name: String, parameters: Vec<String>, body: Vec<Statement> },
    Return(Expression),
    Expression(Expression),
}

#[derive(Debug, Clone)]
pub enum Expression {
    Literal(Value),
    Variable(String),
    List(Vec<Expression>),
    ReadFile(Box<Expression>),
    Length(Box<Expression>),
    At(Box<Expression>, Box<Expression>),
    EnvironmentVariable(Box<Expression>),
    CurrentFolder,
    FileExists(Box<Expression>),
    FolderExists(Box<Expression>),
    ListFiles(Box<Expression>),
    ListFolders(Box<Expression>),
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),
    Join(Box<Expression>, Box<Expression>),
    Call { name: String, arguments: Vec<Expression> },
}

#[derive(Debug, Clone)]
pub enum Condition {
    Truthy(Expression),
    Not(Box<Condition>),
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Contains(Expression, Expression),
    StartsWith(Expression, Expression),
    EndsWith(Expression, Expression),
    Equals(Expression, Expression),
    NotEquals(Expression, Expression),
    GreaterThan(Expression, Expression),
    LessThan(Expression, Expression),
}

#[derive(Debug)]
pub struct BasisError {
    pub line: usize,
    pub message: String,
}

impl BasisError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self { line, message: message.into() }
    }
}

impl fmt::Display for BasisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for BasisError {}

pub fn parse(source: &str) -> Result<Program, BasisError> {
    let lines: Vec<(usize, &str)> = source
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let mut cursor = 0;
    let (statements, _) = parse_block(&lines, &mut cursor, false, false)?;
    Ok(Program { statements })
}

fn parse_block(lines: &[(usize, &str)], cursor: &mut usize, nested: bool, stop_at_otherwise: bool) -> Result<(Vec<Statement>, bool), BasisError> {
    let mut statements = Vec::new();
    while *cursor < lines.len() {
        let (line_number, line) = lines[*cursor];
        if line == "end" {
            if !nested {
                return Err(BasisError::new(line_number, "unexpected end"));
            }
            *cursor += 1;
            return Ok((statements, false));
        }
        if line == "otherwise, do" {
            if !stop_at_otherwise {
                return Err(BasisError::new(line_number, "`otherwise` must belong to a `when` block"));
            }
            *cursor += 1;
            return Ok((statements, true));
        }
        if let Some(rest) = line.strip_prefix("set ") {
            if let Some(rest) = rest.strip_prefix("environment variable ") {
                let (name, value) = split_phrase(rest, " to ").ok_or_else(|| BasisError::new(line_number, "expected `set environment variable name to value`"))?;
                statements.push(Statement::SetEnvironment { name: parse_expression(name, line_number)?, value: parse_expression(value, line_number)? });
            } else {
                let (name, expression) = rest.split_once(" to ").ok_or_else(|| BasisError::new(line_number, "expected `set name to value`"))?;
                statements.push(Statement::Set { name: name.trim().to_string(), value: parse_expression(expression, line_number)? });
            }
            *cursor += 1;
        } else if let Some(expression) = line.strip_prefix("say ") {
            statements.push(Statement::Say(parse_expression(expression, line_number)?));
            *cursor += 1;
        } else if let Some(application) = line.strip_prefix("run ") {
            let (application, arguments) = if let Some((application, arguments)) = split_phrase(application, " with ") {
                let arguments = split_top_level(arguments, ',').into_iter().filter(|argument| !argument.trim().is_empty()).map(|argument| parse_expression(argument, line_number)).collect::<Result<_, _>>()?;
                (application.trim(), arguments)
            } else {
                (application.trim(), Vec::new())
            };
            if application.is_empty() {
                return Err(BasisError::new(line_number, "expected an application after `run`"));
            }
            statements.push(Statement::Run { application: application.to_string(), arguments });
            *cursor += 1;
        } else if let Some(path) = line.strip_prefix("create folder ") {
            statements.push(Statement::CreateFolder(parse_expression(path, line_number)?));
            *cursor += 1;
        } else if let Some(rest) = line.strip_prefix("copy ") {
            let (source, destination) = split_phrase(rest, " to ").ok_or_else(|| BasisError::new(line_number, "expected `copy source to destination`"))?;
            statements.push(Statement::Copy { source: parse_expression(source, line_number)?, destination: parse_expression(destination, line_number)? });
            *cursor += 1;
        } else if let Some(rest) = line.strip_prefix("move ") {
            let (source, destination) = split_phrase(rest, " to ").ok_or_else(|| BasisError::new(line_number, "expected `move source to destination`"))?;
            statements.push(Statement::Move { source: parse_expression(source, line_number)?, destination: parse_expression(destination, line_number)? });
            *cursor += 1;
        } else if let Some(path) = line.strip_prefix("delete file ") {
            statements.push(Statement::DeleteFile(parse_expression(path, line_number)?));
            *cursor += 1;
        } else if let Some(path) = line.strip_prefix("delete folder ") {
            statements.push(Statement::DeleteFolder(parse_expression(path, line_number)?));
            *cursor += 1;
        } else if let Some(rest) = line.strip_prefix("write ") {
            let (content, path) = split_phrase(rest, " to file ").ok_or_else(|| BasisError::new(line_number, "expected `write content to file path`"))?;
            statements.push(Statement::WriteFile { content: parse_expression(content, line_number)?, path: parse_expression(path, line_number)? });
            *cursor += 1;
        } else if let Some(command) = line.strip_prefix("shell ") {
            statements.push(Statement::Shell(parse_expression(command, line_number)?));
            *cursor += 1;
        } else if let Some(command) = line.strip_prefix("start shell ") {
            statements.push(Statement::StartShell(parse_expression(command, line_number)?));
            *cursor += 1;
        } else if let Some(path) = line.strip_prefix("open file ") {
            statements.push(Statement::OpenFile(parse_expression(path, line_number)?));
            *cursor += 1;
        } else if let Some(path) = line.strip_prefix("include ") {
            statements.push(Statement::Include(parse_expression(path, line_number)?));
            *cursor += 1;
        } else if line == "stop" {
            statements.push(Statement::Stop);
            *cursor += 1;
        } else if line == "skip" {
            statements.push(Statement::Skip);
            *cursor += 1;
        } else if let Some(rest) = line.strip_prefix("when ") {
            let (condition, marker) = rest.split_once(", do").ok_or_else(|| BasisError::new(line_number, "expected `when condition, do`"))?;
            if !marker.trim().is_empty() { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let (body, has_otherwise) = parse_block(lines, cursor, true, true)?;
            let otherwise = if has_otherwise {
                let (otherwise, _) = parse_block(lines, cursor, true, false)?;
                Some(otherwise)
            } else {
                None
            };
            statements.push(Statement::When { condition: parse_condition(condition, line_number)?, body, otherwise });
        } else if let Some(rest) = line.strip_prefix("repeat ") {
            let (count, marker) = rest.split_once(" times, do").ok_or_else(|| BasisError::new(line_number, "expected `repeat count times, do`"))?;
            if !marker.trim().is_empty() { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let (body, _) = parse_block(lines, cursor, true, false)?;
            statements.push(Statement::Repeat { count: parse_expression(count, line_number)?, body });
        } else if let Some(rest) = line.strip_prefix("while ") {
            let (condition, marker) = rest.split_once(", do").ok_or_else(|| BasisError::new(line_number, "expected `while condition, do`"))?;
            if !marker.trim().is_empty() { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let (body, _) = parse_block(lines, cursor, true, false)?;
            statements.push(Statement::While { condition: parse_condition(condition, line_number)?, body });
        } else if let Some(rest) = line.strip_prefix("for each ") {
            let (name, rest) = rest.split_once(" in ").ok_or_else(|| BasisError::new(line_number, "expected `for each item in collection, do`"))?;
            let (iterable, marker) = rest.split_once(", do").ok_or_else(|| BasisError::new(line_number, "expected `for each item in collection, do`"))?;
            if !marker.trim().is_empty() { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let (body, _) = parse_block(lines, cursor, true, false)?;
            statements.push(Statement::ForEach { name: name.trim().to_string(), iterable: parse_expression(iterable, line_number)?, body });
        } else if let Some(rest) = line.strip_prefix("return ") {
            statements.push(Statement::Return(parse_expression(rest, line_number)?));
            *cursor += 1;
        } else if let Some(rest) = line.strip_prefix("define ") {
            let (header, marker) = rest.split_once(", do").ok_or_else(|| BasisError::new(line_number, "expected `define name using arguments, do`"))?;
            let (name, args) = header.split_once(" using ").unwrap_or((header, ""));
            let parameters = if args.trim().is_empty() { Vec::new() } else { args.split(',').map(|arg| arg.trim().to_string()).collect() };
            if marker.trim() != "" { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let (body, _) = parse_block(lines, cursor, true, false)?;
            statements.push(Statement::Define { name: name.trim().to_string(), parameters, body });
        } else {
            statements.push(Statement::Expression(parse_expression(line, line_number)?));
            *cursor += 1;
        }
    }
    if nested { return Err(BasisError::new(lines.last().map(|line| line.0).unwrap_or(1), "missing `end`")); }
    Ok((statements, false))
}

fn parse_expression(source: &str, line: usize) -> Result<Expression, BasisError> {
    let source = source.trim();
    if let Some((left, right)) = split_phrase(source, " joined with ") {
        return Ok(Expression::Join(Box::new(parse_expression(left, line)?), Box::new(parse_expression(right, line)?)));
    }
    if let Some((left, right)) = split_phrase(source, " plus ") {
        return Ok(Expression::Add(Box::new(parse_expression(left, line)?), Box::new(parse_expression(right, line)?)));
    }
    if let Some((left, right)) = split_phrase(source, " minus ") {
        return Ok(Expression::Subtract(Box::new(parse_expression(left, line)?), Box::new(parse_expression(right, line)?)));
    }
    if let Some((left, right)) = split_phrase(source, " divided by ") {
        return Ok(Expression::Divide(Box::new(parse_expression(left, line)?), Box::new(parse_expression(right, line)?)));
    }
    if let Some((left, right)) = split_phrase(source, " times ") {
        return Ok(Expression::Multiply(Box::new(parse_expression(left, line)?), Box::new(parse_expression(right, line)?)));
    }
    if source.starts_with('"') && source.ends_with('"') && source.len() >= 2 {
        return Ok(Expression::Literal(Value::Text(unescape_text(&source[1..source.len() - 1]))));
    }
    if source == "true" { return Ok(Expression::Literal(Value::Boolean(true))); }
    if source == "false" { return Ok(Expression::Literal(Value::Boolean(false))); }
    if source == "nothing" { return Ok(Expression::Literal(Value::Nothing)); }
    if let Ok(number) = source.parse::<f64>() { return Ok(Expression::Literal(Value::Number(number))); }
    if let Some(path) = source.strip_prefix("read file ") {
        return Ok(Expression::ReadFile(Box::new(parse_expression(path, line)?)));
    }
    if let Some(value) = source.strip_prefix("length of ") {
        return Ok(Expression::Length(Box::new(parse_expression(value, line)?)));
    }
    if let Some(name) = source.strip_prefix("environment variable ") {
        return Ok(Expression::EnvironmentVariable(Box::new(parse_expression(name, line)?)));
    }
    if source == "current folder" {
        return Ok(Expression::CurrentFolder);
    }
    if let Some(path) = source.strip_prefix("file exists ") {
        return Ok(Expression::FileExists(Box::new(parse_expression(path, line)?)));
    }
    if let Some(path) = source.strip_prefix("folder exists ") {
        return Ok(Expression::FolderExists(Box::new(parse_expression(path, line)?)));
    }
    if let Some(path) = source.strip_prefix("list files in ") {
        return Ok(Expression::ListFiles(Box::new(parse_expression(path, line)?)));
    }
    if let Some(path) = source.strip_prefix("list folders in ") {
        return Ok(Expression::ListFolders(Box::new(parse_expression(path, line)?)));
    }
    if let Some((value, index)) = split_phrase(source, " at ") {
        return Ok(Expression::At(Box::new(parse_expression(value, line)?), Box::new(parse_expression(index, line)?)));
    }
    if source.starts_with('[') && source.ends_with(']') && source.len() >= 2 {
        let contents = &source[1..source.len() - 1];
        let values = split_top_level(contents, ',').into_iter().filter(|value| !value.trim().is_empty()).map(|value| parse_expression(value, line)).collect::<Result<_, _>>()?;
        return Ok(Expression::List(values));
    }
    if let Some((name, args)) = source.split_once(" using ") {
        let arguments = if args.trim().is_empty() { Vec::new() } else { args.split(',').map(|arg| parse_expression(arg, line)).collect::<Result<_, _>>()? };
        return Ok(Expression::Call { name: name.trim().to_string(), arguments });
    }
    if source.chars().all(|character| character.is_alphanumeric() || character == '_') {
        return Ok(Expression::Variable(source.to_string()));
    }
    Err(BasisError::new(line, format!("cannot understand expression `{source}`")))
}

fn unescape_text(source: &str) -> String {
    let mut text = String::new();
    let mut escaped = false;
    for character in source.chars() {
        if escaped {
            text.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            text.push(character);
        }
    }
    if escaped { text.push('\\'); }
    text
}

fn parse_condition(source: &str, line: usize) -> Result<Condition, BasisError> {
    let source = source.trim();
    if let Some((left, right)) = split_phrase(source, " or ") {
        return Ok(Condition::Or(Box::new(parse_condition(left, line)?), Box::new(parse_condition(right, line)?)));
    }
    if let Some((left, right)) = split_phrase(source, " and ") {
        return Ok(Condition::And(Box::new(parse_condition(left, line)?), Box::new(parse_condition(right, line)?)));
    }
    if let Some(source) = source.strip_prefix("not ") {
        return Ok(Condition::Not(Box::new(parse_condition(source, line)?)));
    }
    if let Some((left, right)) = split_phrase(source, " contains ") {
        return Ok(Condition::Contains(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = split_phrase(source, " starts with ") {
        return Ok(Condition::StartsWith(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = split_phrase(source, " ends with ") {
        return Ok(Condition::EndsWith(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = split_phrase(source, " is not ") {
        return Ok(Condition::NotEquals(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = split_phrase(source, " is greater than ") {
        return Ok(Condition::GreaterThan(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = split_phrase(source, " is less than ") {
        return Ok(Condition::LessThan(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = split_phrase(source, " is ") {
        return Ok(Condition::Equals(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    Ok(Condition::Truthy(parse_expression(source, line)?))
}

fn split_top_level(source: &str, separator: char) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in source.char_indices() {
        if escaped { escaped = false; continue; }
        if character == '\\' && quote.is_some() { escaped = true; continue; }
        if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && (character == '\'' || character == '"') {
            quote = Some(character);
        } else if quote.is_none() && character == '[' {
            depth += 1;
        } else if quote.is_none() && character == ']' {
            depth -= 1;
        } else if quote.is_none() && depth == 0 && character == separator {
            pieces.push(&source[start..index]);
            start = index + character.len_utf8();
        }
    }
    pieces.push(&source[start..]);
    pieces
}

fn split_phrase<'a>(source: &'a str, phrase: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in source.char_indices() {
        if escaped { escaped = false; continue; }
        if character == '\\' && quote.is_some() { escaped = true; continue; }
        if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && (character == '\'' || character == '"') {
            quote = Some(character);
        } else if quote.is_none() && character == '[' {
            depth += 1;
        } else if quote.is_none() && character == ']' {
            depth -= 1;
        }
        if quote.is_none() && depth == 0 && source[index..].starts_with(phrase) {
            return Some((&source[..index], &source[index + phrase.len()..]));
        }
    }
    None
}

#[derive(Clone)]
struct Function { parameters: Vec<String>, body: Vec<Statement> }

struct Environment {
    variables: HashMap<String, Value>,
    functions: HashMap<String, Function>,
}

impl Environment {
    fn new() -> Self { Self { variables: HashMap::new(), functions: HashMap::new() } }
}

pub fn run(program: &Program) -> Result<Vec<String>, BasisError> {
    let mut environment = Environment::new();
    let mut output = Vec::new();
    match execute_block(&program.statements, &mut environment, &mut output, 0)? {
        Flow::Next => {}
        Flow::Return(_) => return Err(BasisError::new(0, "return can only be used inside a function")),
        Flow::Break => return Err(BasisError::new(0, "stop can only be used inside a loop")),
        Flow::Continue => return Err(BasisError::new(0, "skip can only be used inside a loop")),
    }
    Ok(output)
}

#[derive(Debug, Clone)]
enum Flow {
    Next,
    Return(Value),
    Break,
    Continue,
}

fn execute_block(statements: &[Statement], environment: &mut Environment, output: &mut Vec<String>, line: usize) -> Result<Flow, BasisError> {
    for statement in statements {
        match statement {
            Statement::Set { name, value } => { let value = evaluate(value, environment, output)?; environment.variables.insert(name.clone(), value); }
            Statement::SetEnvironment { name, value } => {
                let name = value_as_text(name, environment, output)?;
                let value = value_as_text(value, environment, output)?;
                env::set_var(name, value);
            }
            Statement::Say(expression) => {
                let value = evaluate(expression, environment, output)?;
                output.push(value.to_string());
            }
            Statement::Run { application, arguments } => {
                let arguments = arguments.iter().map(|argument| evaluate(argument, environment, output).map(|value| value.to_string())).collect::<Result<_, _>>()?;
                launch_application(application, arguments)?;
            }
            Statement::CreateFolder(path) => {
                let path = value_as_path(path, environment, output)?;
                fs::create_dir_all(&path).map_err(|error| BasisError::new(0, format!("could not create folder `{}`: {error}", path.display())))?;
            }
            Statement::Copy { source, destination } => {
                let source = value_as_path(source, environment, output)?;
                let destination = value_as_path(destination, environment, output)?;
                fs::copy(&source, &destination).map_err(|error| BasisError::new(0, format!("could not copy `{}` to `{}`: {error}", source.display(), destination.display())))?;
            }
            Statement::Move { source, destination } => {
                let source = value_as_path(source, environment, output)?;
                let destination = value_as_path(destination, environment, output)?;
                fs::rename(&source, &destination).map_err(|error| BasisError::new(0, format!("could not move `{}` to `{}`: {error}", source.display(), destination.display())))?;
            }
            Statement::DeleteFile(path) => {
                let path = value_as_path(path, environment, output)?;
                fs::remove_file(&path).map_err(|error| BasisError::new(0, format!("could not delete file `{}`: {error}", path.display())))?;
            }
            Statement::DeleteFolder(path) => {
                let path = value_as_path(path, environment, output)?;
                fs::remove_dir_all(&path).map_err(|error| BasisError::new(0, format!("could not delete folder `{}`: {error}", path.display())))?;
            }
            Statement::WriteFile { content, path } => {
                let content = value_as_text(content, environment, output)?;
                let path = value_as_path(path, environment, output)?;
                fs::write(&path, content).map_err(|error| BasisError::new(0, format!("could not write file `{}`: {error}", path.display())))?;
            }
            Statement::Shell(command) => {
                let command = value_as_text(command, environment, output)?;
                run_shell(command, output)?;
            }
            Statement::StartShell(command) => {
                let command = value_as_text(command, environment, output)?;
                start_shell(command)?;
            }
            Statement::OpenFile(path) => {
                let path = value_as_path(path, environment, output)?;
                open_file(path)?;
            }
            Statement::Include(path) => {
                let path = value_as_path(path, environment, output)?;
                let source = fs::read_to_string(&path).map_err(|error| BasisError::new(0, format!("could not include `{}`: {error}", path.display())))?;
                let program = parse(&source)?;
                let flow = execute_block(&program.statements, environment, output, line)?;
                if !matches!(&flow, Flow::Next) { return Ok(flow); }
            }
            Statement::Stop => return Ok(Flow::Break),
            Statement::Skip => return Ok(Flow::Continue),
            Statement::When { condition, body, otherwise } => {
                if evaluate_condition(condition, environment, output)? {
                    let flow = execute_block(body, environment, output, line)?;
                    if !matches!(&flow, Flow::Next) { return Ok(flow); }
                } else if let Some(otherwise) = otherwise {
                    let flow = execute_block(otherwise, environment, output, line)?;
                    if !matches!(&flow, Flow::Next) { return Ok(flow); }
                }
            }
            Statement::Repeat { count, body } => {
                let count = evaluate_repeat_count(count, environment, output)?;
                for _ in 0..count {
                    match execute_block(body, environment, output, line)? {
                        Flow::Next | Flow::Continue => {}
                        Flow::Break => break,
                        flow @ Flow::Return(_) => return Ok(flow),
                    }
                }
            }
            Statement::While { condition, body } => {
                while evaluate_condition(condition, environment, output)? {
                    match execute_block(body, environment, output, line)? {
                        Flow::Next | Flow::Continue => {}
                        Flow::Break => break,
                        flow @ Flow::Return(_) => return Ok(flow),
                    }
                }
            }
            Statement::ForEach { name, iterable, body } => {
                let values = match evaluate(iterable, environment, output)? {
                    Value::List(values) => values,
                    other => return Err(BasisError::new(0, format!("cannot iterate over {other}"))),
                };
                for value in values {
                    environment.variables.insert(name.clone(), value);
                    match execute_block(body, environment, output, line)? {
                        Flow::Next | Flow::Continue => {}
                        Flow::Break => break,
                        flow @ Flow::Return(_) => return Ok(flow),
                    }
                }
            }
            Statement::Define { name, parameters, body } => { environment.functions.insert(name.clone(), Function { parameters: parameters.clone(), body: body.clone() }); }
            Statement::Return(expression) => return Ok(Flow::Return(evaluate(expression, environment, output)?)),
            Statement::Expression(expression) => { evaluate(expression, environment, output)?; }
        }
    }
    let _ = line;
    Ok(Flow::Next)
}

fn evaluate(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>) -> Result<Value, BasisError> {
    match expression {
        Expression::Literal(Value::Text(value)) => Ok(Value::Text(interpolate_text(value, environment)?)),
        Expression::Literal(value) => Ok(value.clone()),
        Expression::Variable(name) => {
            if let Some(value) = environment.variables.get(name).cloned() {
                Ok(value)
            } else if let Some(function) = environment.functions.get(name).cloned() {
                if !function.parameters.is_empty() {
                    return Err(BasisError::new(0, format!("function `{name}` expects {} arguments", function.parameters.len())));
                }
                let mut local = Environment { variables: environment.variables.clone(), functions: environment.functions.clone() };
                function_result(execute_block(&function.body, &mut local, output, 0)?)
            } else {
                Err(BasisError::new(0, format!("unknown variable or function `{name}`")))
            }
        }
        Expression::List(expressions) => Ok(Value::List(expressions.iter().map(|expression| evaluate(expression, environment, output)).collect::<Result<_, _>>()?)),
        Expression::ReadFile(path) => {
            let path = value_as_path(path, environment, output)?;
            Ok(Value::Text(fs::read_to_string(&path).map_err(|error| BasisError::new(0, format!("could not read file `{}`: {error}", path.display())))?))
        }
        Expression::Length(value) => {
            let value = evaluate(value, environment, output)?;
            let length = match value {
                Value::Text(value) => value.chars().count(),
                Value::List(values) => values.len(),
                _ => return Err(BasisError::new(0, "length requires text or a list")),
            };
            Ok(Value::Number(length as f64))
        }
        Expression::At(value, index) => {
            let value = evaluate(value, environment, output)?;
            let index = evaluate_repeat_count(index, environment, output)?;
            match value {
                Value::List(values) => values.get(index).cloned().ok_or_else(|| BasisError::new(0, format!("list index {index} is out of bounds"))),
                Value::Text(value) => value.chars().nth(index).map(|character| Value::Text(character.to_string())).ok_or_else(|| BasisError::new(0, format!("text index {index} is out of bounds"))),
                _ => Err(BasisError::new(0, "at requires text or a list")),
            }
        }
        Expression::EnvironmentVariable(name) => {
            let name = value_as_text(name, environment, output)?;
            Ok(env::var(name).map(Value::Text).unwrap_or(Value::Nothing))
        }
        Expression::CurrentFolder => Ok(Value::Text(env::current_dir().map_err(|error| BasisError::new(0, format!("could not get current folder: {error}")))?.display().to_string())),
        Expression::FileExists(path) => {
            let path = value_as_path(path, environment, output)?;
            Ok(Value::Boolean(fs::metadata(path).map(|metadata| metadata.is_file()).unwrap_or(false)))
        }
        Expression::FolderExists(path) => {
            let path = value_as_path(path, environment, output)?;
            Ok(Value::Boolean(fs::metadata(path).map(|metadata| metadata.is_dir()).unwrap_or(false)))
        }
        Expression::ListFiles(path) => list_directory(path, environment, output, false),
        Expression::ListFolders(path) => list_directory(path, environment, output, true),
        Expression::Add(left, right) => numeric_operation(left, right, environment, output, |left, right| left + right),
        Expression::Subtract(left, right) => numeric_operation(left, right, environment, output, |left, right| left - right),
        Expression::Multiply(left, right) => numeric_operation(left, right, environment, output, |left, right| left * right),
        Expression::Divide(left, right) => {
            let right_value = evaluate(right, environment, output)?;
            let Value::Number(right_value) = right_value else { return Err(BasisError::new(0, "division requires numbers")); };
            if right_value == 0.0 { return Err(BasisError::new(0, "cannot divide by zero")); }
            let left_value = evaluate(left, environment, output)?;
            let Value::Number(left_value) = left_value else { return Err(BasisError::new(0, "division requires numbers")); };
            Ok(Value::Number(left_value / right_value))
        }
        Expression::Join(left, right) => Ok(Value::Text(format!("{}{}", evaluate(left, environment, output)?, evaluate(right, environment, output)?))),
        Expression::Call { name, arguments } => {
            let function = environment.functions.get(name).cloned().ok_or_else(|| BasisError::new(0, format!("unknown function `{name}`")))?;
            if function.parameters.len() != arguments.len() { return Err(BasisError::new(0, format!("function `{name}` expects {} arguments", function.parameters.len()))); }
            let mut local = Environment { variables: environment.variables.clone(), functions: environment.functions.clone() };
            for (parameter, argument) in function.parameters.iter().zip(arguments) { local.variables.insert(parameter.clone(), evaluate(argument, environment, output)?); }
            function_result(execute_block(&function.body, &mut local, output, 0)?)
        }
    }
}

fn function_result(flow: Flow) -> Result<Value, BasisError> {
    match flow {
        Flow::Next => Ok(Value::Nothing),
        Flow::Return(value) => Ok(value),
        Flow::Break => Err(BasisError::new(0, "stop cannot leave a function")),
        Flow::Continue => Err(BasisError::new(0, "skip cannot leave a function")),
    }
}

fn value_as_text(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>) -> Result<String, BasisError> {
    match evaluate(expression, environment, output)? {
        Value::Text(value) => Ok(value),
        value => Err(BasisError::new(0, format!("expected text, got {value}"))),
    }
}

fn interpolate_text(template: &str, environment: &Environment) -> Result<String, BasisError> {
    let mut result = String::new();
    let mut remaining = template;
    loop {
        let Some(start) = remaining.find('{') else {
            result.push_str(remaining);
            break;
        };
        result.push_str(&remaining[..start]);
        let after_start = &remaining[start + 1..];
        let Some(end) = after_start.find('}') else {
            result.push_str(&remaining[start..]);
            break;
        };
        let name = after_start[..end].trim();
        let value = environment.variables.get(name).ok_or_else(|| BasisError::new(0, format!("unknown interpolation variable `{name}`")))?;
        result.push_str(&value.to_string());
        remaining = &after_start[end + 1..];
    }
    Ok(result)
}

fn value_as_path(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>) -> Result<PathBuf, BasisError> {
    Ok(expand_path(&value_as_text(expression, environment, output)?))
}

fn expand_path(path: &str) -> PathBuf {
    if path == "~" || path.starts_with("~/") {
        if let Some(home) = env::var_os("HOME") {
            let suffix = path.strip_prefix('~').unwrap_or_default().trim_start_matches('/');
            return PathBuf::from(home).join(suffix);
        }
    }
    PathBuf::from(path)
}

fn list_directory(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>, folders: bool) -> Result<Value, BasisError> {
    let path = value_as_path(expression, environment, output)?;
    let mut paths = fs::read_dir(&path)
        .map_err(|error| BasisError::new(0, format!("could not list folder `{}`: {error}", path.display())))?
        .flatten()
        .filter_map(|entry| {
            let entry_path = entry.path();
            let is_match = entry_path.is_dir() == folders;
            is_match.then(|| Value::Text(entry_path.display().to_string()))
        })
        .collect::<Vec<_>>();
    paths.sort_by_key(|value| value.to_string());
    Ok(Value::List(paths))
}

fn run_shell(command: String, output: &mut Vec<String>) -> Result<(), BasisError> {
    let result = Command::new("sh").arg("-c").arg(&command).output().map_err(|error| BasisError::new(0, format!("could not run shell command: {error}")))?;
    output.extend(String::from_utf8_lossy(&result.stdout).lines().map(str::to_string));
    if !result.status.success() {
        let error = String::from_utf8_lossy(&result.stderr).trim().to_string();
        let detail = if error.is_empty() { format!("exit status {}", result.status) } else { error };
        return Err(BasisError::new(0, format!("shell command failed: {detail}")));
    }
    Ok(())
}

fn start_shell(command: String) -> Result<(), BasisError> {
    Command::new("sh").arg("-c").arg(&command).spawn().map_err(|error| BasisError::new(0, format!("could not start shell command: {error}")))?;
    Ok(())
}

fn open_file(path: PathBuf) -> Result<(), BasisError> {
    Command::new("xdg-open").arg(&path).spawn().map_err(|error| BasisError::new(0, format!("could not open `{}`: {error}", path.display())))?;
    Ok(())
}

fn numeric_operation<F>(left: &Expression, right: &Expression, environment: &mut Environment, output: &mut Vec<String>, operation: F) -> Result<Value, BasisError>
where
    F: Fn(f64, f64) -> f64,
{
    let left = evaluate(left, environment, output)?;
    let right = evaluate(right, environment, output)?;
    let (Value::Number(left), Value::Number(right)) = (left, right) else {
        return Err(BasisError::new(0, "arithmetic requires numbers"));
    };
    Ok(Value::Number(operation(left, right)))
}

fn evaluate_condition(condition: &Condition, environment: &mut Environment, output: &mut Vec<String>) -> Result<bool, BasisError> {
    match condition {
        Condition::Truthy(expression) => Ok(is_truthy(&evaluate(expression, environment, output)?)),
        Condition::Not(condition) => Ok(!evaluate_condition(condition, environment, output)?),
        Condition::And(left, right) => Ok(evaluate_condition(left, environment, output)? && evaluate_condition(right, environment, output)?),
        Condition::Or(left, right) => Ok(evaluate_condition(left, environment, output)? || evaluate_condition(right, environment, output)?),
        Condition::Contains(left, right) => contains_value(evaluate(left, environment, output)?, evaluate(right, environment, output)?),
        Condition::StartsWith(left, right) => string_predicate(left, right, environment, output, |left, right| left.starts_with(right)),
        Condition::EndsWith(left, right) => string_predicate(left, right, environment, output, |left, right| left.ends_with(right)),
        Condition::Equals(left, right) => Ok(evaluate(left, environment, output)? == evaluate(right, environment, output)?),
        Condition::NotEquals(left, right) => Ok(evaluate(left, environment, output)? != evaluate(right, environment, output)?),
        Condition::GreaterThan(left, right) => compare_values(left, right, environment, output, |ordering| ordering.is_gt()),
        Condition::LessThan(left, right) => compare_values(left, right, environment, output, |ordering| ordering.is_lt()),
    }
}

fn contains_value(left: Value, right: Value) -> Result<bool, BasisError> {
    match (left, right) {
        (Value::Text(left), Value::Text(right)) => Ok(left.contains(&right)),
        (Value::List(values), right) => Ok(values.contains(&right)),
        _ => Err(BasisError::new(0, "contains requires text or a list")),
    }
}

fn string_predicate<F>(left: &Expression, right: &Expression, environment: &mut Environment, output: &mut Vec<String>, predicate: F) -> Result<bool, BasisError>
where
    F: Fn(&str, &str) -> bool,
{
    let left = value_as_text(left, environment, output)?;
    let right = value_as_text(right, environment, output)?;
    Ok(predicate(&left, &right))
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Boolean(value) => *value,
        Value::Number(value) => *value != 0.0,
        Value::Text(value) => !value.is_empty(),
        Value::List(values) => !values.is_empty(),
        Value::Nothing => false,
    }
}

fn compare_values<F>(left: &Expression, right: &Expression, environment: &mut Environment, output: &mut Vec<String>, predicate: F) -> Result<bool, BasisError>
where
    F: Fn(std::cmp::Ordering) -> bool,
{
    let left = evaluate(left, environment, output)?;
    let right = evaluate(right, environment, output)?;
    let ordering = match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.partial_cmp(&right),
        (Value::Text(left), Value::Text(right)) => Some(left.cmp(&right)),
        _ => None,
    }.ok_or_else(|| BasisError::new(0, "values cannot be compared"))?;
    Ok(predicate(ordering))
}

fn evaluate_repeat_count(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>) -> Result<usize, BasisError> {
    let value = evaluate(expression, environment, output)?;
    match value {
        Value::Number(value) if value >= 0.0 && value.fract() == 0.0 => Ok(value as usize),
        other => Err(BasisError::new(0, format!("repeat expects a non-negative whole number, got {other}"))),
    }
}

pub fn run_file(path: impl AsRef<Path>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let source = fs::read_to_string(path)?;
    Ok(run(&parse(&source)?)?)
}

/// Build a standalone native executable by embedding the validated BASISREAD
/// source and runtime in a generated Rust program. This bootstrap backend will
/// be replaced by direct AST code generation once the language core settles.
pub fn compile_source(source: &str, output_path: impl AsRef<Path>, runtime_source: &str) -> Result<(), Box<dyn std::error::Error>> {
    parse(source)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let build_directory = env::temp_dir().join(format!("basisread-build-{}-{timestamp}", std::process::id()));
    fs::create_dir_all(&build_directory)?;
    let runtime_path = build_directory.join("basisread_runtime.rs");
    let main_path = build_directory.join("main.rs");
    fs::write(&runtime_path, runtime_source)?;
    fs::write(&main_path, format!(
        "#[path = \"basisread_runtime.rs\"]\nmod basisread;\n\nconst PROGRAM: &str = {source:?};\n\nfn main() {{\n    match basisread::parse(PROGRAM).and_then(|program| basisread::run(&program)) {{\n        Ok(lines) => for line in lines {{ println!(\"{{line}}\"); }},\n        Err(error) => {{ eprintln!(\"BASISREAD error: {{error}}\"); std::process::exit(1); }}\n    }}\n}}\n"
    ))?;

    let result = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&main_path)
        .arg("-C").arg("opt-level=2")
        .arg("-o").arg(output_path)
        .status()?;
    let cleanup_result = fs::remove_dir_all(&build_directory);
    if !result.success() {
        return Err(format!("rustc failed with status {result}").into());
    }
    cleanup_result?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub path: PathBuf,
}

/// Find a visible application by its desktop-entry name, ID, or filename.
/// Matching is case-insensitive and permits up to two small spelling errors.
pub fn resolve_application(query: &str) -> Result<DesktopEntry, BasisError> {
    resolve_application_in(query, &application_directories())
}

fn resolve_application_in(query: &str, directories: &[PathBuf]) -> Result<DesktopEntry, BasisError> {
    let mut entries_by_id = HashMap::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") { continue; }
            if let Some(desktop_entry) = read_desktop_entry(&path) {
                entries_by_id.entry(desktop_entry.id.clone()).or_insert(desktop_entry);
            }
        }
    }

    let mut candidates = entries_by_id.into_values().filter_map(|desktop_entry| application_match_score(query, &desktop_entry).map(|score| (score, desktop_entry))).collect::<Vec<_>>();

    if candidates.is_empty() {
        return Err(BasisError::new(0, format!("could not find an application matching `{query}`")));
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.name.cmp(&right.1.name)));
    let best_score = candidates[0].0;
    let best: Vec<_> = candidates.into_iter().filter(|candidate| candidate.0 == best_score).collect();
    if best.len() > 1 {
        let names = best.into_iter().map(|(_, entry)| entry.name).collect::<Vec<_>>().join(", ");
        return Err(BasisError::new(0, format!("`{query}` is ambiguous; matches: {names}")));
    }
    Ok(best.into_iter().next().expect("best application exists").1)
}

fn application_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        directories.push(PathBuf::from(data_home).join("applications"));
    } else if let Some(home) = env::var_os("HOME") {
        directories.push(PathBuf::from(home).join(".local/share/applications"));
    }

    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for data_dir in data_dirs.split(':').filter(|data_dir| !data_dir.is_empty()) {
        directories.push(PathBuf::from(data_dir).join("applications"));
    }
    directories
}

fn read_desktop_entry(path: &Path) -> Option<DesktopEntry> {
    let source = fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut entry_type = None;
    let mut hidden = false;
    let mut no_display = false;

    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') { continue; }
        let Some((key, value)) = line.split_once('=') else { continue };
        match key {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(value.trim().to_string()),
            "Type" => entry_type = Some(value.trim()),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            "NoDisplay" => no_display = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if entry_type != Some("Application") || hidden || no_display { return None; }
    let name = name?;
    let exec = exec?;
    let id = path.file_stem()?.to_string_lossy().into_owned();
    Some(DesktopEntry { id, name, exec, path: path.to_path_buf() })
}

fn application_match_score(query: &str, entry: &DesktopEntry) -> Option<usize> {
    let query = normalize_application_name(query);
    if query.is_empty() { return None; }
    let names = [entry.name.as_str(), entry.id.as_str(), entry.path.file_stem()?.to_str()?];
    let best = names.iter().map(|name| levenshtein(&query, &normalize_application_name(name))).min()?;
    let allowed = if query.chars().count() >= 7 { 2 } else { 1 };
    (best <= allowed).then_some(best)
}

fn normalize_application_name(name: &str) -> String {
    name.chars().filter(|character| character.is_alphanumeric()).flat_map(|character| character.to_lowercase()).collect()
}

fn levenshtein(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (left_index, left_character) in left.chars().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_character) in right.iter().enumerate() {
            let cost = usize::from(left_character != *right_character);
            current.push((current[right_index] + 1).min(previous[right_index + 1] + 1).min(previous[right_index] + cost));
        }
        previous = current;
    }
    previous[right.len()]
}

fn launch_application(query: &str, extra_arguments: Vec<String>) -> Result<(), BasisError> {
    let entry = resolve_application(query)?;
    let mut command = parse_exec(&entry.exec)?;
    command.extend(extra_arguments);
    let Some((program, arguments)) = command.split_first() else {
        return Err(BasisError::new(0, format!("desktop entry `{}` has an empty Exec command", entry.name)));
    };
    Command::new(program).args(arguments).spawn().map_err(|error| BasisError::new(0, format!("could not launch `{}`: {error}", entry.name)))?;
    Ok(())
}

fn parse_exec(exec: &str) -> Result<Vec<String>, BasisError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in exec.chars() {
        if escaped { current.push(character); escaped = false; continue; }
        if character == '\\' && quote != Some('\'') { escaped = true; continue; }
        if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && (character == '\'' || character == '"') {
            quote = Some(character);
        } else if quote.is_none() && character.is_whitespace() {
            if !current.is_empty() { words.push(std::mem::take(&mut current)); }
        } else {
            current.push(character);
        }
    }
    if escaped { current.push('\\'); }
    if quote.is_some() { return Err(BasisError::new(0, "unterminated quote in desktop entry Exec command")); }
    if !current.is_empty() { words.push(current); }
    Ok(words.into_iter().filter(|word| !word.starts_with('%')).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runs_variables_and_output() {
        let output = run(&parse("set name to \"Ransom\"\nsay name").unwrap()).unwrap();
        assert_eq!(output, vec!["Ransom"]);
    }

    #[test]
    fn runs_functions_and_joining() {
        let source = r#"
            define greet using person, do
                return "Hello, " joined with person
            end
            say greet using "Ransom"
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["Hello, Ransom"]);
    }

    #[test]
    fn parses_desktop_application_commands() {
        let program = parse("run firefox with \"--private-window\"").unwrap();
        assert!(matches!(program.statements.as_slice(), [Statement::Run { application, arguments }] if application == "firefox" && arguments.len() == 1));
        assert!(matches!(parse("start shell \"long-running-command\"").unwrap().statements.as_slice(), [Statement::StartShell(_)]));
    }

    #[test]
    fn accepts_small_application_name_typos() {
        let entry = DesktopEntry { id: "firefox".into(), name: "Firefox".into(), exec: "firefox %u".into(), path: PathBuf::from("firefox.desktop") };
        assert_eq!(application_match_score("firefokx", &entry), Some(1));
    }

    #[test]
    fn parses_desktop_exec_placeholders() {
        assert_eq!(parse_exec("firefox --new-window %u").unwrap(), vec!["firefox", "--new-window"]);
    }

    #[test]
    fn runs_conditions_and_repetition() {
        let source = r#"
            set score to 5
            when score is 5, do
                say "correct"
            end
            when score is greater than 3, do
                say "high enough"
            end
            when score is 5 and not score is 4, do
                say "combined"
            end
            set filename to "backup.tar"
            when filename ends with ".tar", do
                say "archive"
            end
            when "BASISREAD" contains "READ", do
                say "contains"
            end
            when "this is text" is "this is text", do
                say "quoted condition"
            end
            set names to ["Ada", "Grace"]
            when names contains "Ada", do
                say "list contains"
            end
            when score is less than 3, do
                say "wrong branch"
            otherwise, do
                say "otherwise branch"
            end
            repeat 2 times, do
                say "again"
            end
            while false, do
                say "never"
            end
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["correct", "high enough", "combined", "archive", "contains", "quoted condition", "list contains", "otherwise branch", "again", "again"]);
    }

    #[test]
    fn runs_for_each_over_dynamic_lists() {
        let source = r#"
            set names to ["Ada", "Grace"]
            for each name in names, do
                say name
            end
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["Ada", "Grace"]);
    }

    #[test]
    fn uses_arithmetic_to_drive_a_while_loop() {
        let source = r#"
            set counter to 0
            while counter is less than 3, do
                say counter
                set counter to counter plus 1
            end
            say 1 plus 2 times 3
            say length of ["a", "b"]
            say ["a", "b"] at 1
            say "hello" at 1
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["0", "1", "2", "7", "2", "b", "e"]);
    }

    #[test]
    fn supports_stop_and_skip_in_loops() {
        let source = r#"
            set count to 0
            repeat 5 times, do
                set count to count plus 1
                when count is 2, do
                    skip
                end
                say count
                when count is 4, do
                    stop
                end
            end
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["1", "3", "4"]);
    }

    #[test]
    fn automates_files_and_shell_commands() {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("basisread-{suffix}"));
        let input = root.join("input.txt");
        let copy = root.join("copy.txt");
        let source = format!(
            "create folder \"{}\"\nwrite \"hello\" to file \"{}\"\nwhen folder exists \"{}\", do\n say \"folder present\"\nend\nwhen file exists \"{}\", do\n say read file \"{}\"\nend\ncopy \"{}\" to \"{}\"\nset files to list files in \"{}\"\nfor each file in files, do\n say file\nend\nset environment variable \"BASISREAD_TEST_MODE\" to \"works\"\nshell \"printf $BASISREAD_TEST_MODE\"\ndelete folder \"{}\"",
            root.display(), input.display(), root.display(), input.display(), input.display(), input.display(), copy.display(), root.display(), root.display()
        );
        assert_eq!(run(&parse(&source).unwrap()).unwrap(), vec!["folder present".to_string(), "hello".to_string(), copy.display().to_string(), input.display().to_string(), "works".to_string()]);
        assert!(!root.exists());
    }

    #[test]
    fn functions_can_read_global_values() {
        let source = r#"
            set prefix to "Hello, "
            define greet using person, do
                return prefix joined with person
            end
            say greet using "Ransom"
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["Hello, Ransom"]);
    }

    #[test]
    fn supports_zero_argument_functions() {
        let source = r#"
            define hello, do
                return "Hello"
            end
            say hello
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["Hello"]);
    }

    #[test]
    fn includes_code_in_the_current_environment() {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("basisread-helper-{suffix}.basis"));
        fs::write(&path, "define greet using person, do\n return \"Hello, \" joined with person\nend\n").unwrap();
        let source = format!("include \"{}\"\nsay greet using \"Ransom\"", path.display());
        assert_eq!(run(&parse(&source).unwrap()).unwrap(), vec!["Hello, Ransom"]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn expands_home_directory_paths() {
        if let Some(home) = env::var_os("HOME") {
            assert_eq!(expand_path("~/basisread"), PathBuf::from(home).join("basisread"));
        }
        assert_eq!(expand_path("relative/file"), PathBuf::from("relative/file"));
    }

    #[test]
    fn interpolates_variables_in_text() {
        let source = r#"
            set name to "Ransom"
            say "Hello, {name}!"
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["Hello, Ransom!"]);
    }

    #[test]
    fn unescapes_text_literals() {
        let source = r#"say "line one\nline two\tend""#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["line one\nline two\tend"]);
    }
}
