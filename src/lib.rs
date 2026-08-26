use std::{
    collections::HashMap,
    env,
    fmt,
    fs,
    path::{Path, PathBuf},
    process::Command,
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
    Say(Expression),
    Run { application: String },
    When { condition: Condition, body: Vec<Statement> },
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
        write!(f, "line {}: {}", self.line, self.message)
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
    let statements = parse_block(&lines, &mut cursor, false)?;
    Ok(Program { statements })
}

fn parse_block(lines: &[(usize, &str)], cursor: &mut usize, nested: bool) -> Result<Vec<Statement>, BasisError> {
    let mut statements = Vec::new();
    while *cursor < lines.len() {
        let (line_number, line) = lines[*cursor];
        if line == "end" {
            if !nested {
                return Err(BasisError::new(line_number, "unexpected end"));
            }
            *cursor += 1;
            return Ok(statements);
        }
        if let Some(rest) = line.strip_prefix("set ") {
            let (name, expression) = rest.split_once(" to ").ok_or_else(|| BasisError::new(line_number, "expected `set name to value`"))?;
            statements.push(Statement::Set { name: name.trim().to_string(), value: parse_expression(expression, line_number)? });
            *cursor += 1;
        } else if let Some(expression) = line.strip_prefix("say ") {
            statements.push(Statement::Say(parse_expression(expression, line_number)?));
            *cursor += 1;
        } else if let Some(application) = line.strip_prefix("run ") {
            let application = application.trim();
            if application.is_empty() {
                return Err(BasisError::new(line_number, "expected an application after `run`"));
            }
            statements.push(Statement::Run { application: application.to_string() });
            *cursor += 1;
        } else if let Some(rest) = line.strip_prefix("when ") {
            let (condition, marker) = rest.split_once(", do").ok_or_else(|| BasisError::new(line_number, "expected `when condition, do`"))?;
            if !marker.trim().is_empty() { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let body = parse_block(lines, cursor, true)?;
            statements.push(Statement::When { condition: parse_condition(condition, line_number)?, body });
        } else if let Some(rest) = line.strip_prefix("repeat ") {
            let (count, marker) = rest.split_once(" times, do").ok_or_else(|| BasisError::new(line_number, "expected `repeat count times, do`"))?;
            if !marker.trim().is_empty() { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let body = parse_block(lines, cursor, true)?;
            statements.push(Statement::Repeat { count: parse_expression(count, line_number)?, body });
        } else if let Some(rest) = line.strip_prefix("while ") {
            let (condition, marker) = rest.split_once(", do").ok_or_else(|| BasisError::new(line_number, "expected `while condition, do`"))?;
            if !marker.trim().is_empty() { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let body = parse_block(lines, cursor, true)?;
            statements.push(Statement::While { condition: parse_condition(condition, line_number)?, body });
        } else if let Some(rest) = line.strip_prefix("for each ") {
            let (name, rest) = rest.split_once(" in ").ok_or_else(|| BasisError::new(line_number, "expected `for each item in collection, do`"))?;
            let (iterable, marker) = rest.split_once(", do").ok_or_else(|| BasisError::new(line_number, "expected `for each item in collection, do`"))?;
            if !marker.trim().is_empty() { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let body = parse_block(lines, cursor, true)?;
            statements.push(Statement::ForEach { name: name.trim().to_string(), iterable: parse_expression(iterable, line_number)?, body });
        } else if let Some(rest) = line.strip_prefix("return ") {
            statements.push(Statement::Return(parse_expression(rest, line_number)?));
            *cursor += 1;
        } else if let Some(rest) = line.strip_prefix("define ") {
            let (header, marker) = rest.split_once(", do").ok_or_else(|| BasisError::new(line_number, "expected `define name using arguments, do`"))?;
            let (name, args) = header.split_once(" using ").ok_or_else(|| BasisError::new(line_number, "expected `using` in function definition"))?;
            let parameters = if args.trim().is_empty() { Vec::new() } else { args.split(',').map(|arg| arg.trim().to_string()).collect() };
            if marker.trim() != "" { return Err(BasisError::new(line_number, "unexpected text after `do`")); }
            *cursor += 1;
            let body = parse_block(lines, cursor, true)?;
            statements.push(Statement::Define { name: name.trim().to_string(), parameters, body });
        } else {
            statements.push(Statement::Expression(parse_expression(line, line_number)?));
            *cursor += 1;
        }
    }
    if nested { return Err(BasisError::new(lines.last().map(|line| line.0).unwrap_or(1), "missing `end`")); }
    Ok(statements)
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
        return Ok(Expression::Literal(Value::Text(source[1..source.len() - 1].replace("\\\"", "\""))));
    }
    if source == "true" { return Ok(Expression::Literal(Value::Boolean(true))); }
    if source == "false" { return Ok(Expression::Literal(Value::Boolean(false))); }
    if source == "nothing" { return Ok(Expression::Literal(Value::Nothing)); }
    if let Ok(number) = source.parse::<f64>() { return Ok(Expression::Literal(Value::Number(number))); }
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

fn parse_condition(source: &str, line: usize) -> Result<Condition, BasisError> {
    let source = source.trim();
    if let Some((left, right)) = source.split_once(" is not ") {
        return Ok(Condition::NotEquals(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = source.split_once(" is greater than ") {
        return Ok(Condition::GreaterThan(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = source.split_once(" is less than ") {
        return Ok(Condition::LessThan(parse_expression(left, line)?, parse_expression(right, line)?));
    }
    if let Some((left, right)) = source.split_once(" is ") {
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
    execute_block(&program.statements, &mut environment, &mut output, 0)?;
    Ok(output)
}

fn execute_block(statements: &[Statement], environment: &mut Environment, output: &mut Vec<String>, line: usize) -> Result<Option<Value>, BasisError> {
    for statement in statements {
        match statement {
            Statement::Set { name, value } => { let value = evaluate(value, environment, output)?; environment.variables.insert(name.clone(), value); }
            Statement::Say(expression) => {
                let value = evaluate(expression, environment, output)?;
                output.push(value.to_string());
            }
            Statement::Run { application } => { launch_application(application)?; }
            Statement::When { condition, body } => {
                if evaluate_condition(condition, environment, output)? {
                    if let Some(value) = execute_block(body, environment, output, line)? { return Ok(Some(value)); }
                }
            }
            Statement::Repeat { count, body } => {
                let count = evaluate_repeat_count(count, environment, output)?;
                for _ in 0..count {
                    if let Some(value) = execute_block(body, environment, output, line)? { return Ok(Some(value)); }
                }
            }
            Statement::While { condition, body } => {
                while evaluate_condition(condition, environment, output)? {
                    if let Some(value) = execute_block(body, environment, output, line)? { return Ok(Some(value)); }
                }
            }
            Statement::ForEach { name, iterable, body } => {
                let values = match evaluate(iterable, environment, output)? {
                    Value::List(values) => values,
                    other => return Err(BasisError::new(0, format!("cannot iterate over {other}"))),
                };
                for value in values {
                    environment.variables.insert(name.clone(), value);
                    if let Some(value) = execute_block(body, environment, output, line)? { return Ok(Some(value)); }
                }
            }
            Statement::Define { name, parameters, body } => { environment.functions.insert(name.clone(), Function { parameters: parameters.clone(), body: body.clone() }); }
            Statement::Return(expression) => return Ok(Some(evaluate(expression, environment, output)?)),
            Statement::Expression(expression) => { evaluate(expression, environment, output)?; }
        }
    }
    let _ = line;
    Ok(None)
}

fn evaluate(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>) -> Result<Value, BasisError> {
    match expression {
        Expression::Literal(value) => Ok(value.clone()),
        Expression::Variable(name) => environment.variables.get(name).cloned().ok_or_else(|| BasisError::new(0, format!("unknown variable `{name}`"))),
        Expression::List(expressions) => Ok(Value::List(expressions.iter().map(|expression| evaluate(expression, environment, output)).collect::<Result<_, _>>()?)),
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
            let mut local = Environment { variables: HashMap::new(), functions: environment.functions.clone() };
            for (parameter, argument) in function.parameters.iter().zip(arguments) { local.variables.insert(parameter.clone(), evaluate(argument, environment, output)?); }
            Ok(execute_block(&function.body, &mut local, output, 0)?.unwrap_or(Value::Nothing))
        }
    }
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
        Condition::Equals(left, right) => Ok(evaluate(left, environment, output)? == evaluate(right, environment, output)?),
        Condition::NotEquals(left, right) => Ok(evaluate(left, environment, output)? != evaluate(right, environment, output)?),
        Condition::GreaterThan(left, right) => compare_values(left, right, environment, output, |ordering| ordering.is_gt()),
        Condition::LessThan(left, right) => compare_values(left, right, environment, output, |ordering| ordering.is_lt()),
    }
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
    let mut candidates = Vec::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") { continue; }
            if let Some(desktop_entry) = read_desktop_entry(&path) {
                if let Some(score) = application_match_score(query, &desktop_entry) {
                    candidates.push((score, desktop_entry));
                }
            }
        }
    }

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

fn launch_application(query: &str) -> Result<(), BasisError> {
    let entry = resolve_application(query)?;
    let command = parse_exec(&entry.exec)?;
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
        let program = parse("run firefox").unwrap();
        assert!(matches!(program.statements.as_slice(), [Statement::Run { application }] if application == "firefox"));
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
            repeat 2 times, do
                say "again"
            end
            while false, do
                say "never"
            end
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["correct", "high enough", "again", "again"]);
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
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["0", "1", "2", "7"]);
    }
}
