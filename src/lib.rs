use std::{collections::HashMap, fmt, fs, path::Path};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    Boolean(bool),
    Nothing,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(value) => write!(f, "{value}"),
            Self::Number(value) if value.fract() == 0.0 => write!(f, "{value:.0}"),
            Self::Number(value) => write!(f, "{value}"),
            Self::Boolean(value) => write!(f, "{value}"),
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
    Define { name: String, parameters: Vec<String>, body: Vec<Statement> },
    Return(Expression),
    Expression(Expression),
}

#[derive(Debug, Clone)]
pub enum Expression {
    Literal(Value),
    Variable(String),
    Join(Box<Expression>, Box<Expression>),
    Call { name: String, arguments: Vec<Expression> },
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
    if let Some((left, right)) = source.split_once(" joined with ") {
        return Ok(Expression::Join(Box::new(parse_expression(left, line)?), Box::new(parse_expression(right, line)?)));
    }
    if source.starts_with('"') && source.ends_with('"') && source.len() >= 2 {
        return Ok(Expression::Literal(Value::Text(source[1..source.len() - 1].replace("\\\"", "\""))));
    }
    if source == "true" { return Ok(Expression::Literal(Value::Boolean(true))); }
    if source == "false" { return Ok(Expression::Literal(Value::Boolean(false))); }
    if let Ok(number) = source.parse::<f64>() { return Ok(Expression::Literal(Value::Number(number))); }
    if let Some((name, args)) = source.split_once(" using ") {
        let arguments = if args.trim().is_empty() { Vec::new() } else { args.split(',').map(|arg| parse_expression(arg, line)).collect::<Result<_, _>>()? };
        return Ok(Expression::Call { name: name.trim().to_string(), arguments });
    }
    if source.chars().all(|character| character.is_alphanumeric() || character == '_') {
        return Ok(Expression::Variable(source.to_string()));
    }
    Err(BasisError::new(line, format!("cannot understand expression `{source}`")))
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
            Statement::Say(expression) => output.push(evaluate(expression, environment, output)?.to_string()),
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

pub fn run_file(path: impl AsRef<Path>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let source = fs::read_to_string(path)?;
    Ok(run(&parse(&source)?)?)
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
}
