use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, VecDeque},
    env,
    fmt,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub mod lexer;
pub mod parser;
mod codegen;
pub use lexer::{lex, Span, Token, TokenKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    Boolean(bool),
    List(Vec<Value>),
    Object(BTreeMap<String, Value>),
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
            Self::Object(values) => {
                let values = values.iter().map(|(key, value)| format!("{key}: {value}")).collect::<Vec<_>>().join(", ");
                write!(f, "{{{values}}}")
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
    SetPath { path: Vec<String>, value: Expression },
    SetEnvironment { name: Expression, value: Expression },
    SeedRandom(Expression),
    Say(Expression),
    SayColored { expression: Expression, color: String },
    MoveCursor { x: Expression, y: Expression },
    HideCursor,
    ShowCursor,
    Run { application: String, arguments: Vec<Expression> },
    CreateFolder(Expression),
    Copy { source: Expression, destination: Expression },
    Move { source: Expression, destination: Expression },
    DeleteFile(Expression),
    DeleteFolder(Expression),
    WriteFile { content: Expression, path: Expression },
    AppendFile { content: Expression, path: Expression },
    Save { value: Expression, path: Expression },
    Wait(Expression),
    ClearTerminal,
    DrawText { text: Expression, x: Expression, y: Expression },
    ClearScreenBuffer,
    RenderScreen,
    ResizeScreen { width: Expression, height: Expression },
    ListAdd { path: Vec<String>, value: Expression },
    ListRemove { path: Vec<String>, value: Expression },
    Shell(Expression),
    StartShell(Expression),
    OpenFile(Expression),
    Include(Expression),
    Stop,
    Skip,
    When { condition: Condition, body: Vec<Statement>, otherwise: Option<Vec<Statement>> },
    Try { body: Vec<Statement>, otherwise: Vec<Statement> },
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
    Object(Vec<(String, Expression)>),
    Field(Box<Expression>, String),
    Ask(Box<Expression>),
    AskKey,
    ReadFile(Box<Expression>),
    LoadFile(Box<Expression>),
    Length(Box<Expression>),
    At(Box<Expression>, Box<Expression>),
    KeyAvailable,
    EnvironmentVariable(Box<Expression>),
    CurrentFolder,
    FileExists(Box<Expression>),
    FolderExists(Box<Expression>),
    ListFiles(Box<Expression>),
    ListFolders(Box<Expression>),
    ListApplications,
    TerminalWidth,
    TerminalHeight,
    ScreenWidth,
    ScreenHeight,
    Timer,
    RandomNumber { lower: Option<Box<Expression>>, upper: Option<Box<Expression>>, integer: bool },
    RandomChoice(Box<Expression>),
    Minimum(Box<Expression>, Box<Expression>),
    Maximum(Box<Expression>, Box<Expression>),
    Clamp { value: Box<Expression>, lower: Box<Expression>, upper: Box<Expression> },
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
    pub column: usize,
    pub message: String,
}

impl BasisError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self { line, column: 0, message: message.into() }
    }

    fn at(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self { line, column, message: message.into() }
    }
}

impl fmt::Display for BasisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else if self.column == 0 {
            write!(f, "line {}: {}", self.line, self.message)
        } else {
            write!(f, "line {}, column {}: {}", self.line, self.column, self.message)
        }
    }
}

impl std::error::Error for BasisError {}

pub fn parse(source: &str) -> Result<Program, BasisError> {
    parser::parse(source)
}

#[derive(Clone)]
struct Function { parameters: Vec<String>, body: Vec<Statement> }

#[derive(Clone)]
struct RandomState {
    state: u64,
}

impl RandomState {
    fn new() -> Self {
        let time_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0);
        Self { state: time_seed ^ (std::process::id() as u64) | 1 }
    }

    fn seed(&mut self, seed: u64) {
        self.state = seed | 1;
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.state = value;
        value.wrapping_mul(2_685_821_657_736_338_717)
    }

    fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1_u64 << 53) as f64)
    }
}

#[derive(Clone)]
struct ScreenBuffer {
    width: usize,
    height: usize,
    cells: Vec<Vec<char>>,
}

impl ScreenBuffer {
    fn from_terminal() -> Self {
        let width = screen_dimension_size(terminal_dimension("COLUMNS", "cols", 80.0), 80);
        let height = screen_dimension_size(terminal_dimension("LINES", "lines", 24.0), 24);
        Self::new(width, height)
    }

    fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self { width, height, cells: vec![vec![' '; width]; height] }
    }

    fn resize(&mut self, width: usize, height: usize) {
        *self = Self::new(width, height);
    }

    fn clear(&mut self) {
        for row in &mut self.cells {
            row.fill(' ');
        }
    }

    fn draw_text(&mut self, text: &str, x: usize, y: usize) {
        if x == 0 || y == 0 { return; }
        let mut row = y - 1;
        let start_column = x - 1;
        let mut column = start_column;
        for character in text.chars() {
            if character == '\n' {
                row += 1;
                column = start_column;
                continue;
            }
            if row >= self.height { break; }
            if column < self.width {
                self.cells[row][column] = character;
            }
            column += 1;
        }
    }

    fn lines(&self) -> Vec<String> {
        self.cells.iter().map(|row| row.iter().collect()).collect()
    }
}

struct Environment {
    variables: HashMap<String, Value>,
    functions: HashMap<String, Function>,
    interactive: bool,
    input: Rc<RefCell<VecDeque<String>>>,
    pending_keys: Rc<RefCell<VecDeque<String>>>,
    random: Rc<RefCell<RandomState>>,
    screen: Rc<RefCell<ScreenBuffer>>,
    started: Instant,
}

impl Environment {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
            functions: HashMap::new(),
            interactive: false,
            input: Rc::new(RefCell::new(VecDeque::new())),
            pending_keys: Rc::new(RefCell::new(VecDeque::new())),
            random: Rc::new(RefCell::new(RandomState::new())),
            screen: Rc::new(RefCell::new(ScreenBuffer::from_terminal())),
            started: Instant::now(),
        }
    }

    fn with_input(input: &[&str]) -> Self {
        let mut environment = Self::new();
        environment.input = Rc::new(RefCell::new(input.iter().map(|value| (*value).to_string()).collect()));
        environment
    }

    fn interactive() -> Self {
        let mut environment = Self::new();
        environment.interactive = true;
        environment
    }

    fn child(&self) -> Self {
        Self {
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            interactive: self.interactive,
            input: Rc::clone(&self.input),
            pending_keys: Rc::clone(&self.pending_keys),
            random: Rc::clone(&self.random),
            screen: Rc::clone(&self.screen),
            started: self.started,
        }
    }
}

pub fn run(program: &Program) -> Result<Vec<String>, BasisError> {
    let mut environment = Environment::new();
    let mut output = Vec::new();
    finish_run(execute_block(&program.statements, &mut environment, &mut output, 0)?, output)
}

pub fn run_with_input(program: &Program, input: &[&str]) -> Result<Vec<String>, BasisError> {
    let mut environment = Environment::with_input(input);
    let mut output = Vec::new();
    finish_run(execute_block(&program.statements, &mut environment, &mut output, 0)?, output)
}

pub fn run_interactive(program: &Program) -> Result<(), BasisError> {
    let mut environment = Environment::interactive();
    let mut output = Vec::new();
    finish_run(execute_block(&program.statements, &mut environment, &mut output, 0)?, output).map(|_| ())
}

fn finish_run(flow: Flow, output: Vec<String>) -> Result<Vec<String>, BasisError> {
    match flow {
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
            Statement::SetPath { path, value } => {
                let value = evaluate(value, environment, output)?;
                set_environment_path(&mut environment.variables, path, value)?;
            }
            Statement::SetEnvironment { name, value } => {
                let name = value_as_text(name, environment, output)?;
                let value = value_as_text(value, environment, output)?;
                env::set_var(name, value);
            }
            Statement::SeedRandom(seed) => {
                let seed = evaluate_number(seed, environment, output)?;
                if seed < 0.0 || seed.fract() != 0.0 || !seed.is_finite() {
                    return Err(BasisError::new(0, "random seed requires a finite non-negative whole number"));
                }
                environment.random.borrow_mut().seed(seed as u64);
            }
            Statement::Say(expression) => {
                let value = evaluate(expression, environment, output)?;
                let text = value.to_string();
                if environment.interactive {
                    println!("{text}");
                }
                output.push(text);
            }
            Statement::SayColored { expression, color } => {
                let text = evaluate(expression, environment, output)?.to_string();
                if environment.interactive {
                    print!("{}{}\x1b[0m\n", color_code(color), text);
                    io::stdout().flush().map_err(|error| BasisError::new(0, format!("could not show colored text: {error}")))?;
                }
                output.push(text);
            }
            Statement::MoveCursor { x, y } => {
                let x = terminal_coordinate(x, environment, output, "x")?;
                let y = terminal_coordinate(y, environment, output, "y")?;
                if environment.interactive {
                    print!("\x1b[{y};{x}H");
                    io::stdout().flush().map_err(|error| BasisError::new(0, format!("could not position cursor: {error}")))?;
                }
            }
            Statement::HideCursor => {
                if environment.interactive {
                    print!("\x1b[?25l");
                    io::stdout().flush().map_err(|error| BasisError::new(0, format!("could not hide cursor: {error}")))?;
                }
            }
            Statement::ShowCursor => {
                if environment.interactive {
                    print!("\x1b[?25h");
                    io::stdout().flush().map_err(|error| BasisError::new(0, format!("could not show cursor: {error}")))?;
                }
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
                copy_path(&source, &destination).map_err(|error| BasisError::new(0, format!("could not copy `{}` to `{}`: {error}", source.display(), destination.display())))?;
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
            Statement::AppendFile { content, path } => {
                let content = value_as_text(content, environment, output)?;
                let path = value_as_path(path, environment, output)?;
                let mut file = fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|error| BasisError::new(0, format!("could not open file `{}` for appending: {error}", path.display())))?;
                file.write_all(content.as_bytes()).map_err(|error| BasisError::new(0, format!("could not append to file `{}`: {error}", path.display())))?;
            }
            Statement::Save { value, path } => {
                let value = evaluate(value, environment, output)?;
                let path = value_as_path(path, environment, output)?;
                fs::write(&path, serialize_value(&value)).map_err(|error| BasisError::new(0, format!("could not save state to `{}`: {error}", path.display())))?;
            }
            Statement::Wait(seconds) => {
                let seconds = evaluate_number(seconds, environment, output)?;
                if seconds < 0.0 || !seconds.is_finite() {
                    return Err(BasisError::new(0, "wait requires a finite non-negative number of seconds"));
                }
                thread::sleep(Duration::from_secs_f64(seconds));
            }
            Statement::ClearTerminal => {
                if environment.interactive {
                    print!("\x1b[2J\x1b[H");
                    io::stdout().flush().map_err(|error| BasisError::new(0, format!("could not clear terminal: {error}")))?;
                }
            }
            Statement::DrawText { text, x, y } => {
                let text = value_as_text(text, environment, output)?;
                let x = terminal_coordinate(x, environment, output, "x")?;
                let y = terminal_coordinate(y, environment, output, "y")?;
                environment.screen.borrow_mut().draw_text(&text, x, y);
            }
            Statement::ClearScreenBuffer => {
                environment.screen.borrow_mut().clear();
            }
            Statement::RenderScreen => {
                render_screen(environment, output)?;
            }
            Statement::ResizeScreen { width, height } => {
                let width = screen_coordinate(width, environment, output, "width")?;
                let height = screen_coordinate(height, environment, output, "height")?;
                environment.screen.borrow_mut().resize(width, height);
            }
            Statement::ListAdd { path, value } => {
                let value = evaluate(value, environment, output)?;
                list_add(&mut environment.variables, path, value)?;
            }
            Statement::ListRemove { path, value } => {
                let value = evaluate(value, environment, output)?;
                list_remove(&mut environment.variables, path, &value)?;
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
            Statement::Try { body, otherwise } => {
                match execute_block(body, environment, output, line) {
                    Ok(flow) => {
                        if !matches!(&flow, Flow::Next) { return Ok(flow); }
                    }
                    Err(_) => {
                        let flow = execute_block(otherwise, environment, output, line)?;
                        if !matches!(&flow, Flow::Next) { return Ok(flow); }
                    }
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
                let mut local = environment.child();
                function_result(execute_block(&function.body, &mut local, output, 0)?)
            } else {
                Err(BasisError::new(0, format!("unknown variable or function `{name}`")))
            }
        }
        Expression::List(expressions) => Ok(Value::List(expressions.iter().map(|expression| evaluate(expression, environment, output)).collect::<Result<_, _>>()?)),
        Expression::Object(entries) => {
            let mut values = BTreeMap::new();
            for (key, expression) in entries {
                values.insert(key.clone(), evaluate(expression, environment, output)?);
            }
            Ok(Value::Object(values))
        }
        Expression::Field(value, field) => {
            let value = evaluate(value, environment, output)?;
            match value {
                Value::Object(values) => values.get(field).cloned().ok_or_else(|| BasisError::new(0, format!("object has no field `{field}`"))),
                other => Err(BasisError::new(0, format!("cannot read field `{field}` from {other}"))),
            }
        }
        Expression::Ask(prompt) => ask_value(prompt, environment, output),
        Expression::AskKey => ask_key_value(environment),
        Expression::ReadFile(path) => {
            let path = value_as_path(path, environment, output)?;
            Ok(Value::Text(fs::read_to_string(&path).map_err(|error| BasisError::new(0, format!("could not read file `{}`: {error}", path.display())))?))
        }
        Expression::LoadFile(path) => {
            let path = value_as_path(path, environment, output)?;
            let source = fs::read_to_string(&path).map_err(|error| BasisError::new(0, format!("could not load state from `{}`: {error}", path.display())))?;
            deserialize_value(&source).map_err(|error| BasisError::new(0, format!("could not load state from `{}`: {error}", path.display())))
        }
        Expression::Length(value) => {
            let value = evaluate(value, environment, output)?;
            let length = match value {
                Value::Text(value) => value.chars().count(),
                Value::List(values) => values.len(),
                Value::Object(values) => values.len(),
                _ => return Err(BasisError::new(0, "length requires text, a list, or an object")),
            };
            Ok(Value::Number(length as f64))
        }
        Expression::At(value, index) => {
            let value = evaluate(value, environment, output)?;
            let index_value = evaluate(index, environment, output)?;
            match (value, index_value) {
                (Value::List(values), Value::Number(index)) if index >= 0.0 && index.fract() == 0.0 => {
                    let index = index as usize;
                    values.get(index).cloned().ok_or_else(|| BasisError::new(0, format!("list index {index} is out of bounds")))
                }
                (Value::Text(value), Value::Number(index)) if index >= 0.0 && index.fract() == 0.0 => {
                    let index = index as usize;
                    value.chars().nth(index).map(|character| Value::Text(character.to_string())).ok_or_else(|| BasisError::new(0, format!("text index {index} is out of bounds")))
                }
                (Value::Object(values), Value::Text(key)) => values.get(&key).cloned().ok_or_else(|| BasisError::new(0, format!("object has no key `{key}`"))),
                (Value::List(_) | Value::Text(_), other) => Err(BasisError::new(0, format!("index requires a non-negative whole number, got {other}"))),
                (Value::Object(_), other) => Err(BasisError::new(0, format!("object access requires a text key, got {other}"))),
                (_, _) => Err(BasisError::new(0, "at requires text, a list, or an object")),
            }
        }
        Expression::KeyAvailable => key_available_value(environment),
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
        Expression::ListApplications => Ok(Value::List(list_applications().into_iter().map(|entry| Value::Text(entry.name)).collect())),
        Expression::TerminalWidth => Ok(Value::Number(terminal_dimension("COLUMNS", "cols", 80.0))),
        Expression::TerminalHeight => Ok(Value::Number(terminal_dimension("LINES", "lines", 24.0))),
        Expression::ScreenWidth => Ok(Value::Number(environment.screen.borrow().width as f64)),
        Expression::ScreenHeight => Ok(Value::Number(environment.screen.borrow().height as f64)),
        Expression::Timer => Ok(Value::Number(environment.started.elapsed().as_secs_f64())),
        Expression::RandomNumber { lower, upper, integer } => random_number(lower.as_deref(), upper.as_deref(), *integer, environment, output),
        Expression::RandomChoice(value) => {
            let value = evaluate(value, environment, output)?;
            let Value::List(values) = value else { return Err(BasisError::new(0, "random choice requires a list")); };
            if values.is_empty() {
                return Err(BasisError::new(0, "random choice cannot choose from an empty list"));
            }
            let index = (environment.random.borrow_mut().next_unit() * values.len() as f64) as usize;
            Ok(values[index.min(values.len() - 1)].clone())
        }
        Expression::Minimum(left, right) => numeric_extreme(left, right, environment, output, f64::min),
        Expression::Maximum(left, right) => numeric_extreme(left, right, environment, output, f64::max),
        Expression::Clamp { value, lower, upper } => {
            let value = evaluate_number(value, environment, output)?;
            let lower = evaluate_number(lower, environment, output)?;
            let upper = evaluate_number(upper, environment, output)?;
            if lower > upper {
                return Err(BasisError::new(0, "clamp lower bound cannot be greater than upper bound"));
            }
            Ok(Value::Number(value.clamp(lower, upper)))
        }
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
            let mut local = environment.child();
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

fn evaluate_number(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>) -> Result<f64, BasisError> {
    match evaluate(expression, environment, output)? {
        Value::Number(value) => Ok(value),
        value => Err(BasisError::new(0, format!("expected a number, got {value}"))),
    }
}

fn ask_value(prompt: &Expression, environment: &mut Environment, output: &mut Vec<String>) -> Result<Value, BasisError> {
    let prompt = value_as_text(prompt, environment, output)?;
    if environment.interactive {
        print!("{prompt}");
        io::stdout().flush().map_err(|error| BasisError::new(0, format!("could not show input prompt: {error}")))?;
    }
    let answer = if let Some(answer) = environment.input.borrow_mut().pop_front() {
        answer
    } else if environment.interactive {
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).map_err(|error| BasisError::new(0, format!("could not read input: {error}")))?;
        answer.trim_end_matches(['\r', '\n']).to_string()
    } else {
        return Err(BasisError::new(0, "ask needs input; use run_interactive or run_with_input"));
    };
    Ok(Value::Text(answer))
}

fn ask_key_value(environment: &mut Environment) -> Result<Value, BasisError> {
    if let Some(answer) = environment.pending_keys.borrow_mut().pop_front() {
        return Ok(Value::Text(answer));
    }
    if let Some(answer) = environment.input.borrow_mut().pop_front() {
        return Ok(Value::Text(answer.chars().next().map(|character| character.to_string()).unwrap_or_default()));
    }
    if !environment.interactive {
        return Err(BasisError::new(0, "ask key needs input; use run_interactive or run_with_input"));
    }

    terminal_key(environment, true)?.ok_or_else(|| BasisError::new(0, "could not read key"))
}

fn key_available_value(environment: &mut Environment) -> Result<Value, BasisError> {
    if !environment.pending_keys.borrow().is_empty() {
        return Ok(Value::Boolean(true));
    }
    if !environment.interactive {
        return Ok(Value::Boolean(!environment.input.borrow().is_empty()));
    }

    if let Some(key) = terminal_key(environment, false)? {
        environment.pending_keys.borrow_mut().push_back(key);
        Ok(Value::Boolean(true))
    } else {
        Ok(Value::Boolean(false))
    }
}

fn terminal_key(environment: &mut Environment, wait_for_key: bool) -> Result<Option<String>, BasisError> {
    if !environment.interactive {
        return Ok(None);
    }

    let saved = Command::new("stty")
        .arg("-g")
        .output()
        .map_err(|error| BasisError::new(0, format!("could not inspect terminal mode: {error}")))?;
    if !saved.status.success() {
        if !wait_for_key {
            return Ok(None);
        }
        return Err(BasisError::new(0, "could not inspect terminal mode"));
    }
    let saved_mode = String::from_utf8_lossy(&saved.stdout).trim().to_string();
    let minimum = if wait_for_key { "1" } else { "0" };
    let mode = Command::new("stty")
        .args(["-icanon", "-echo", "min", minimum, "time", "0"])
        .status()
        .map_err(|error| BasisError::new(0, format!("could not enable single-key input: {error}")))?;
    if !mode.success() {
        return Err(BasisError::new(0, "could not enable single-key input"));
    }

    let mut byte = [0_u8; 1];
    let read_result = if wait_for_key {
        io::stdin().read_exact(&mut byte).map(|_| 1)
    } else {
        io::stdin().read(&mut byte)
    };
    let restore_result = Command::new("stty").arg(&saved_mode).status();
    let count = read_result.map_err(|error| BasisError::new(0, format!("could not read key: {error}")))?;
    let restored = restore_result.map_err(|error| BasisError::new(0, format!("could not restore terminal mode: {error}")))?;
    if !restored.success() {
        return Err(BasisError::new(0, "could not restore terminal mode"));
    }
    if count == 0 {
        Ok(None)
    } else {
        Ok(Some(String::from_utf8_lossy(&byte[..count]).to_string()))
    }
}

fn color_code(color: &str) -> &'static str {
    match color.to_ascii_lowercase().as_str() {
        "black" => "\x1b[30m",
        "red" => "\x1b[31m",
        "green" => "\x1b[32m",
        "yellow" => "\x1b[33m",
        "blue" => "\x1b[34m",
        "magenta" => "\x1b[35m",
        "cyan" => "\x1b[36m",
        "white" => "\x1b[37m",
        "gray" | "grey" => "\x1b[90m",
        "bright black" => "\x1b[90m",
        "bright red" => "\x1b[91m",
        "bright green" => "\x1b[92m",
        "bright yellow" => "\x1b[93m",
        "bright blue" => "\x1b[94m",
        "bright magenta" => "\x1b[95m",
        "bright cyan" => "\x1b[96m",
        "bright white" => "\x1b[97m",
        _ => "\x1b[0m",
    }
}

fn terminal_coordinate(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>, axis: &str) -> Result<usize, BasisError> {
    let value = evaluate_number(expression, environment, output)?;
    if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > usize::MAX as f64 {
        return Err(BasisError::new(0, format!("cursor {axis} coordinate must be a positive whole number")));
    }
    Ok(value as usize)
}

fn screen_coordinate(expression: &Expression, environment: &mut Environment, output: &mut Vec<String>, axis: &str) -> Result<usize, BasisError> {
    let value = evaluate_number(expression, environment, output)?;
    if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > 10_000.0 {
        return Err(BasisError::new(0, format!("screen {axis} must be a positive whole number below 10000")));
    }
    Ok(value as usize)
}

fn screen_dimension_size(value: f64, fallback: usize) -> usize {
    if value.is_finite() && value >= 1.0 && value <= 10_000.0 { value as usize } else { fallback }
}

fn render_screen(environment: &mut Environment, output: &mut Vec<String>) -> Result<(), BasisError> {
    let lines = environment.screen.borrow().lines();
    if environment.interactive {
        print!("\x1b[2J\x1b[H{}\n", lines.join("\n"));
        io::stdout().flush().map_err(|error| BasisError::new(0, format!("could not render screen: {error}")))?;
    } else {
        output.extend(lines);
    }
    Ok(())
}

fn terminal_dimension(environment_name: &str, command_name: &str, fallback: f64) -> f64 {
    if let Some(value) = env::var(environment_name).ok().and_then(|value| value.parse::<f64>().ok()).filter(|value| *value > 0.0) {
        return value;
    }
    if let Ok(output) = Command::new("tput").arg(command_name).output() {
        if let Ok(value) = String::from_utf8_lossy(&output.stdout).trim().parse::<f64>() {
            if value > 0.0 { return value; }
        }
    }
    fallback
}

fn random_number(
    lower: Option<&Expression>,
    upper: Option<&Expression>,
    integer: bool,
    environment: &mut Environment,
    output: &mut Vec<String>,
) -> Result<Value, BasisError> {
    let (Some(lower), Some(upper)) = (lower, upper) else {
        if lower.is_some() || upper.is_some() {
            return Err(BasisError::new(0, "random number needs both a lower and upper bound"));
        }
        return Ok(Value::Number(environment.random.borrow_mut().next_unit()));
    };
    let lower = evaluate_number(lower, environment, output)?;
    let upper = evaluate_number(upper, environment, output)?;
    if !lower.is_finite() || !upper.is_finite() || lower > upper {
        return Err(BasisError::new(0, "random number bounds must be finite and ordered"));
    }
    if integer {
        if lower.fract() != 0.0 || upper.fract() != 0.0 || upper - lower > u64::MAX as f64 - 1.0 {
            return Err(BasisError::new(0, "random integer bounds must be whole numbers in a practical range"));
        }
        let low = lower as i128;
        let high = upper as i128;
        let span = (high - low + 1) as u128;
        let random = (environment.random.borrow_mut().next_u64() as u128) % span;
        return Ok(Value::Number((low as f64) + random as f64));
    }
    Ok(Value::Number(lower + (upper - lower) * environment.random.borrow_mut().next_unit()))
}

fn numeric_extreme(
    left: &Expression,
    right: &Expression,
    environment: &mut Environment,
    output: &mut Vec<String>,
    operation: fn(f64, f64) -> f64,
) -> Result<Value, BasisError> {
    let left = evaluate_number(left, environment, output)?;
    let right = evaluate_number(right, environment, output)?;
    Ok(Value::Number(operation(left, right)))
}

fn lookup_environment_path(variables: &HashMap<String, Value>, path: &[String]) -> Option<Value> {
    let (root, fields) = path.split_first()?;
    let mut value = variables.get(root)?.clone();
    for field in fields {
        let Value::Object(values) = value else { return None; };
        value = values.get(field)?.clone();
    }
    Some(value)
}

fn set_environment_path(variables: &mut HashMap<String, Value>, path: &[String], value: Value) -> Result<(), BasisError> {
    let (root, fields) = path.split_first().ok_or_else(|| BasisError::new(0, "assignment needs a variable name"))?;
    if fields.is_empty() {
        variables.insert(root.clone(), value);
        return Ok(());
    }
    let target = variables.get_mut(root).ok_or_else(|| BasisError::new(0, format!("unknown variable `{root}`")))?;
    set_nested_value(target, fields, value)
}

fn set_nested_value(target: &mut Value, fields: &[String], value: Value) -> Result<(), BasisError> {
    if fields.is_empty() {
        *target = value;
        return Ok(());
    }
    let Value::Object(values) = target else {
        return Err(BasisError::new(0, format!("cannot assign field `{}` on a non-object", fields[0])));
    };
    let child = values.get_mut(&fields[0]).ok_or_else(|| BasisError::new(0, format!("object has no field `{}`", fields[0])))?;
    set_nested_value(child, &fields[1..], value)
}

fn nested_value_mut<'a>(target: &'a mut Value, fields: &[String]) -> Result<&'a mut Value, BasisError> {
    if fields.is_empty() {
        return Ok(target);
    }
    let Value::Object(values) = target else {
        return Err(BasisError::new(0, format!("cannot access field `{}` on a non-object", fields[0])));
    };
    let child = values.get_mut(&fields[0]).ok_or_else(|| BasisError::new(0, format!("object has no field `{}`", fields[0])))?;
    nested_value_mut(child, &fields[1..])
}

fn list_add(variables: &mut HashMap<String, Value>, path: &[String], value: Value) -> Result<(), BasisError> {
    let (root, fields) = path.split_first().ok_or_else(|| BasisError::new(0, "list add needs a variable name"))?;
    let target = variables.get_mut(root).ok_or_else(|| BasisError::new(0, format!("unknown variable `{root}`")))?;
    let target = nested_value_mut(target, fields)?;
    let Value::List(values) = target else { return Err(BasisError::new(0, "add requires a list")); };
    values.push(value);
    Ok(())
}

fn list_remove(variables: &mut HashMap<String, Value>, path: &[String], value: &Value) -> Result<(), BasisError> {
    let (root, fields) = path.split_first().ok_or_else(|| BasisError::new(0, "list remove needs a variable name"))?;
    let target = variables.get_mut(root).ok_or_else(|| BasisError::new(0, format!("unknown variable `{root}`")))?;
    let target = nested_value_mut(target, fields)?;
    let Value::List(values) = target else { return Err(BasisError::new(0, "remove requires a list")); };
    if let Some(index) = values.iter().position(|candidate| candidate == value) {
        values.remove(index);
    }
    Ok(())
}

fn serialize_value(value: &Value) -> String {
    match value {
        Value::Text(value) => format!("\"{}\"", escape_state_text(value)),
        Value::Number(value) if value.is_finite() => value.to_string(),
        Value::Number(_) => "null".to_string(),
        Value::Boolean(value) => value.to_string(),
        Value::List(values) => format!("[{}]", values.iter().map(serialize_value).collect::<Vec<_>>().join(",")),
        Value::Object(values) => format!("{{{}}}", values.iter().map(|(key, value)| format!("\"{}\":{}", escape_state_text(key), serialize_value(value))).collect::<Vec<_>>().join(",")),
        Value::Nothing => "null".to_string(),
    }
}

fn escape_state_text(value: &str) -> String {
    value.chars().flat_map(|character| match character {
        '"' => "\\\"".chars().collect::<Vec<_>>(),
        '\\' => "\\\\".chars().collect::<Vec<_>>(),
        '\n' => "\\n".chars().collect::<Vec<_>>(),
        '\r' => "\\r".chars().collect::<Vec<_>>(),
        '\t' => "\\t".chars().collect::<Vec<_>>(),
        other => vec![other],
    }).collect()
}

fn deserialize_value(source: &str) -> Result<Value, String> {
    let mut parser = StateParser { characters: source.chars().collect(), cursor: 0 };
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.cursor != parser.characters.len() {
        return Err("unexpected data after saved value".to_string());
    }
    Ok(value)
}

struct StateParser {
    characters: Vec<char>,
    cursor: usize,
}

impl StateParser {
    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_whitespace();
        match self.current() {
            Some('"') => Ok(Value::Text(self.parse_string()?)),
            Some('[') => self.parse_list(),
            Some('{') => self.parse_object(),
            Some('t') if self.consume_word("true") => Ok(Value::Boolean(true)),
            Some('f') if self.consume_word("false") => Ok(Value::Boolean(false)),
            Some('n') if self.consume_word("null") => Ok(Value::Nothing),
            Some('-' | '0'..='9') => self.parse_number(),
            _ => Err("expected a saved value".to_string()),
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if !self.consume_character('"') { return Err("expected a quoted string".to_string()); }
        let mut value = String::new();
        while let Some(character) = self.next() {
            match character {
                '"' => return Ok(value),
                '\\' => {
                    let escaped = self.next().ok_or_else(|| "unterminated saved string".to_string())?;
                    value.push(match escaped { 'n' => '\n', 'r' => '\r', 't' => '\t', '"' => '"', '\\' => '\\', other => other });
                }
                other => value.push(other),
            }
        }
        Err("unterminated saved string".to_string())
    }

    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.cursor;
        while matches!(self.current(), Some(character) if character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E')) { self.cursor += 1; }
        let text: String = self.characters[start..self.cursor].iter().collect();
        text.parse::<f64>().map(Value::Number).map_err(|_| format!("invalid saved number `{text}`"))
    }

    fn parse_list(&mut self) -> Result<Value, String> {
        self.consume_character('[');
        let mut values = Vec::new();
        loop {
            self.skip_whitespace();
            if self.consume_character(']') { return Ok(Value::List(values)); }
            values.push(self.parse_value()?);
            self.skip_whitespace();
            if self.consume_character(']') { return Ok(Value::List(values)); }
            if !self.consume_character(',') { return Err("expected `,` or `]` in saved list".to_string()); }
        }
    }

    fn parse_object(&mut self) -> Result<Value, String> {
        self.consume_character('{');
        let mut values = BTreeMap::new();
        loop {
            self.skip_whitespace();
            if self.consume_character('}') { return Ok(Value::Object(values)); }
            let key = self.parse_string()?;
            self.skip_whitespace();
            if !self.consume_character(':') { return Err("expected `:` in saved object".to_string()); }
            values.insert(key, self.parse_value()?);
            self.skip_whitespace();
            if self.consume_character('}') { return Ok(Value::Object(values)); }
            if !self.consume_character(',') { return Err("expected `,` or `}` in saved object".to_string()); }
        }
    }

    fn consume_word(&mut self, word: &str) -> bool {
        let end = self.cursor + word.chars().count();
        if self.characters.get(self.cursor..end).is_some_and(|characters| characters.iter().collect::<String>() == word) {
            self.cursor = end;
            true
        } else { false }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.current(), Some(character) if character.is_whitespace()) { self.cursor += 1; }
    }

    fn consume_character(&mut self, expected: char) -> bool {
        if self.current() == Some(expected) { self.cursor += 1; true } else { false }
    }

    fn current(&self) -> Option<char> { self.characters.get(self.cursor).copied() }

    fn next(&mut self) -> Option<char> {
        let character = self.current()?;
        self.cursor += 1;
        Some(character)
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
        let path = name.split('.').map(str::to_string).collect::<Vec<_>>();
        let value = lookup_environment_path(&environment.variables, &path).ok_or_else(|| BasisError::new(0, format!("unknown interpolation variable `{name}`")))?;
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

fn copy_path(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = destination.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
    }
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
        (Value::Object(values), Value::Text(key)) => Ok(values.contains_key(&key)),
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
        Value::Object(values) => !values.is_empty(),
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

pub fn run_file_interactive(path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string(path)?;
    run_interactive(&parse(&source)?)?;
    Ok(())
}

/// Build a standalone native executable by embedding the validated BASISREAD
/// source and runtime in a generated Rust program. This bootstrap backend will
/// be replaced by direct AST code generation once the language core settles.
pub fn compile_source(source: &str, output_path: impl AsRef<Path>, runtime_source: &str, lexer_source: &str, parser_source: &str, codegen_source: &str) -> Result<(), Box<dyn std::error::Error>> {
    let program = parse(source)?;
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
    fs::write(build_directory.join("lexer.rs"), lexer_source)?;
    fs::write(build_directory.join("parser.rs"), parser_source)?;
    fs::write(build_directory.join("codegen.rs"), codegen_source)?;
    fs::write(&main_path, codegen::native_main(&program))?;

    let result = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&main_path)
        .arg("-C").arg("opt-level=2")
        .arg("-o").arg(output_path)
        .status()?;
    let cleanup_result = fs::remove_dir_all(&build_directory);
    if !result.success() {
        let _ = cleanup_result;
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

fn list_applications() -> Vec<DesktopEntry> {
    let mut entries_by_id = HashMap::new();
    for directory in application_directories() {
        let Ok(entries) = fs::read_dir(directory) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") { continue; }
            if let Some(desktop_entry) = read_desktop_entry(&path) {
                entries_by_id.entry(desktop_entry.id.clone()).or_insert(desktop_entry);
            }
        }
    }
    let mut applications = entries_by_id.into_values().collect::<Vec<_>>();
    applications.sort_by(|left, right| left.name.cmp(&right.name).then_with(|| left.id.cmp(&right.id)));
    applications
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
    let allowed = if query.chars().count() >= 5 { 2 } else { 1 };
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
    fn lexes_words_text_numbers_and_structure() {
        let tokens = lex("set name to \"Ransom\"\nwhen name is 5, do\nend").unwrap();
        assert_eq!(
            tokens.into_iter().map(|token| token.kind).collect::<Vec<_>>(),
            vec![
                TokenKind::Word("set".into()),
                TokenKind::Word("name".into()),
                TokenKind::Word("to".into()),
                TokenKind::Text("Ransom".into()),
                TokenKind::Newline,
                TokenKind::Word("when".into()),
                TokenKind::Word("name".into()),
                TokenKind::Word("is".into()),
                TokenKind::Number(5.0),
                TokenKind::Comma,
                TokenKind::Word("do".into()),
                TokenKind::Newline,
                TokenKind::Word("end".into()),
                TokenKind::End,
            ]
        );
    }

    #[test]
    fn lexer_ignores_comments_but_preserves_comment_markers_in_text() {
        let tokens = lex("say \"hello # world\" # ignored\n# ignored too\nsay done").unwrap();
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Text("hello # world".into())));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Word("done".into())));
        assert!(!tokens.iter().any(|token| token.kind == TokenKind::Word("ignored".into())));
    }

    #[test]
    fn lexer_decodes_escapes_and_tracks_positions() {
        let tokens = lex("say \"line one\\nline two\"\n    say done").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Text("line one\nline two".into()));
        assert_eq!(tokens[1].span.line, 1);
        assert_eq!(tokens[1].span.column, 5);
        assert_eq!(tokens[4].span.line, 2);
        assert_eq!(tokens[4].span.column, 9);
    }

    #[test]
    fn parser_reports_lexical_string_errors() {
        let error = parse("say \"unterminated").unwrap_err();
        assert_eq!(error.line, 1);
        assert!(error.message.contains("unterminated string literal"));
    }

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
        assert!(matches!(parse("say list applications").unwrap().statements.as_slice(), [Statement::Say(Expression::ListApplications)]));
    }

    #[test]
    fn accepts_small_application_name_typos() {
        let entry = DesktopEntry { id: "firefox".into(), name: "Firefox".into(), exec: "firefox %u".into(), path: PathBuf::from("firefox.desktop") };
        assert_eq!(application_match_score("firefokx", &entry), Some(1));
        assert_eq!(application_match_score("firefzz", &entry), Some(2));
    }

    #[test]
    fn resolves_real_desktop_entries_with_precedence() {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let user_directory = std::env::temp_dir().join(format!("basisread-apps-user-{suffix}"));
        let system_directory = std::env::temp_dir().join(format!("basisread-apps-system-{suffix}"));
        fs::create_dir_all(&user_directory).unwrap();
        fs::create_dir_all(&system_directory).unwrap();
        fs::write(user_directory.join("firefox.desktop"), "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox %u\n").unwrap();
        fs::write(system_directory.join("firefox.desktop"), "[Desktop Entry]\nType=Application\nName=System Firefox\nExec=firefox %u\n").unwrap();
        let resolved = resolve_application_in("firefokx", &[user_directory.clone(), system_directory.clone()]).unwrap();
        assert_eq!(resolved.name, "Firefox");
        assert_eq!(resolved.exec, "firefox %u");
        fs::remove_dir_all(user_directory).unwrap();
        fs::remove_dir_all(system_directory).unwrap();
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
            when "a, do" is "a, do", do
                say "quoted delimiter"
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
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["correct", "high enough", "combined", "archive", "contains", "quoted condition", "quoted delimiter", "list contains", "otherwise branch", "again", "again"]);
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
    fn supports_return_without_a_value() {
        let source = r#"
            define finish, do
                return
                say "unreachable"
            end
            say finish
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["nothing"]);
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

    #[test]
    fn supports_inline_comments_without_touching_text() {
        let source = "say \"hello # world\" # this is a comment\n# another comment\nsay \"done\"";
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["hello # world", "done"]);
    }

    #[test]
    fn copies_directories_recursively() {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let source = std::env::temp_dir().join(format!("basisread-copy-source-{suffix}"));
        let destination = std::env::temp_dir().join(format!("basisread-copy-destination-{suffix}"));
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/value.txt"), "value").unwrap();
        copy_path(&source, &destination).unwrap();
        assert_eq!(fs::read_to_string(destination.join("nested/value.txt")).unwrap(), "value");
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(destination).unwrap();
    }

    #[test]
    fn appends_to_text_files() {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("basisread-append-{suffix}.txt"));
        let source = format!("write \"first\" to file \"{}\"\nappend \" second\" to file \"{}\"\nsay read file \"{}\"\ndelete file \"{}\"", path.display(), path.display(), path.display(), path.display());
        assert_eq!(run(&parse(&source).unwrap()).unwrap(), vec!["first second"]);
        assert!(!path.exists());
    }

    #[test]
    fn supports_game_objects_nested_assignment_and_list_mutation() {
        let source = r#"
            set player to {
                health: 20,
                inventory: []
            }
            set player.health to player.health minus 3
            add "torch" to player.inventory
            when player.inventory contains "torch", do
                say "ready"
            end
            say "Health: {player.health}"
            say length of player.inventory
            remove "torch" from player.inventory
            say length of player.inventory
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["ready", "Health: 17", "1", "0"]);
    }

    #[test]
    fn supports_scripted_input_seeded_randomness_and_game_helpers() {
        let source = r#"
            set random seed to 42
            set first to random choice from ["north", "south", "east"]
            set random seed to 42
            set second to random choice from ["north", "south", "east"]
            when first is second, do
                say "seeded"
            end
            set answer to ask "Move? "
            say answer
            say minimum of 4 and 2
            say maximum of 4 and 2
            say clamp 10 between 0 and 5
        "#;
        assert_eq!(run_with_input(&parse(source).unwrap(), &["explore"]).unwrap(), vec!["seeded", "explore", "2", "4", "5"]);
    }

    #[test]
    fn saves_and_loads_dynamic_game_state() {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("basisread-state-{suffix}.json"));
        let source = format!(
            "set state to {{ health: 18, inventory: [\"key\"] }}\nsave state to file \"{}\"\nset state to load file \"{}\"\nsay state.health\nsay state.inventory at 0\ndelete file \"{}\"",
            path.display(), path.display(), path.display()
        );
        assert_eq!(run(&parse(&source).unwrap()).unwrap(), vec!["18", "key"]);
        assert!(!path.exists());
    }

    #[test]
    fn supports_implicit_while_do_error_recovery_and_game_controls() {
        let source = r#"
            set count to 0
            while count is less than 2
                say count
                set count to count plus 1
            end
            try, do
                say missing_value
            else, do
                say "recovered"
            end
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["0", "1", "recovered"]);
    }

    #[test]
    fn supports_terminal_game_controls_without_polluting_buffered_output() {
        let source = r#"
            set key to ask key
            say key
            say "danger" in red
            say terminal width
            say terminal height
            move cursor to 3, 4
            hide cursor
            show cursor
        "#;
        let output = run_with_input(&parse(source).unwrap(), &["x"]).unwrap();
        assert_eq!(output[0..2], ["x", "danger"]);
        assert!(output[2].parse::<f64>().unwrap() > 0.0);
        assert!(output[3].parse::<f64>().unwrap() > 0.0);
    }

    #[test]
    fn polls_scripted_key_input_without_consuming_it() {
        let source = r#"
            set ready to key available
            set key to ask key
            say ready
            say key
        "#;
        assert_eq!(run_with_input(&parse(source).unwrap(), &["x"]).unwrap(), vec!["true", "x"]);
    }

    #[test]
    fn reports_no_key_for_empty_buffered_input() {
        assert_eq!(run(&parse("say key available").unwrap()).unwrap(), vec!["false"]);
    }

    #[test]
    fn draws_and_renders_a_reusable_ascii_screen_buffer() {
        let source = r#"
            resize screen to 8, 3
            draw text "HP" at 5, 1
            draw text "@" at 2, 2
            render screen
            say screen width
            say screen height
        "#;
        assert_eq!(run(&parse(source).unwrap()).unwrap(), vec!["    HP  ", " @      ", "        ", "8", "3"]);
    }

    #[test]
    fn exposes_an_elapsed_timer() {
        let output = run(&parse("say timer").unwrap()).unwrap();
        assert!(output[0].parse::<f64>().unwrap() >= 0.0);
    }
}
