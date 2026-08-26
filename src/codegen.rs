use super::{Condition, Expression, Program, Statement, Value};

pub fn native_main(program: &Program) -> String {
    format!(
        "#[path = \"basisread_runtime.rs\"]\nmod basisread;\n\nfn main() {{\n    let program = {};\n    if let Err(error) = basisread::run_interactive(&program) {{\n        eprintln!(\"BASISREAD error: {{error}}\");\n        std::process::exit(1);\n    }}\n}}\n",
        program_literal(program)
    )
}

fn program_literal(program: &Program) -> String {
    format!("basisread::Program {{ statements: {} }}", statements_literal(&program.statements))
}

fn statements_literal(statements: &[Statement]) -> String {
    format!("vec![{}]", join(statements.iter().map(statement_literal)))
}

fn statement_literal(statement: &Statement) -> String {
    match statement {
        Statement::Set { name, value } => format!("basisread::Statement::Set {{ name: String::from({:?}), value: {} }}", name, expression_literal(value)),
        Statement::SetPath { path, value } => format!("basisread::Statement::SetPath {{ path: {}, value: {} }}", strings_literal(path), expression_literal(value)),
        Statement::SetEnvironment { name, value } => format!("basisread::Statement::SetEnvironment {{ name: {}, value: {} }}", expression_literal(name), expression_literal(value)),
        Statement::SeedRandom(seed) => format!("basisread::Statement::SeedRandom({})", expression_literal(seed)),
        Statement::Say(expression) => format!("basisread::Statement::Say({})", expression_literal(expression)),
        Statement::SayColored { expression, color } => format!("basisread::Statement::SayColored {{ expression: {}, color: String::from({color:?}) }}", expression_literal(expression)),
        Statement::MoveCursor { x, y } => format!("basisread::Statement::MoveCursor {{ x: {}, y: {} }}", expression_literal(x), expression_literal(y)),
        Statement::HideCursor => "basisread::Statement::HideCursor".to_string(),
        Statement::ShowCursor => "basisread::Statement::ShowCursor".to_string(),
        Statement::Run { application, arguments } => format!("basisread::Statement::Run {{ application: String::from({:?}), arguments: {} }}", application, expressions_literal(arguments)),
        Statement::CreateFolder(path) => format!("basisread::Statement::CreateFolder({})", expression_literal(path)),
        Statement::Copy { source, destination } => format!("basisread::Statement::Copy {{ source: {}, destination: {} }}", expression_literal(source), expression_literal(destination)),
        Statement::Move { source, destination } => format!("basisread::Statement::Move {{ source: {}, destination: {} }}", expression_literal(source), expression_literal(destination)),
        Statement::DeleteFile(path) => format!("basisread::Statement::DeleteFile({})", expression_literal(path)),
        Statement::DeleteFolder(path) => format!("basisread::Statement::DeleteFolder({})", expression_literal(path)),
        Statement::WriteFile { content, path } => format!("basisread::Statement::WriteFile {{ content: {}, path: {} }}", expression_literal(content), expression_literal(path)),
        Statement::AppendFile { content, path } => format!("basisread::Statement::AppendFile {{ content: {}, path: {} }}", expression_literal(content), expression_literal(path)),
        Statement::Save { value, path } => format!("basisread::Statement::Save {{ value: {}, path: {} }}", expression_literal(value), expression_literal(path)),
        Statement::Wait(seconds) => format!("basisread::Statement::Wait({})", expression_literal(seconds)),
        Statement::ClearTerminal => "basisread::Statement::ClearTerminal".to_string(),
        Statement::DrawText { text, x, y } => format!("basisread::Statement::DrawText {{ text: {}, x: {}, y: {} }}", expression_literal(text), expression_literal(x), expression_literal(y)),
        Statement::ClearScreenBuffer => "basisread::Statement::ClearScreenBuffer".to_string(),
        Statement::RenderScreen => "basisread::Statement::RenderScreen".to_string(),
        Statement::ResizeScreen { width, height } => format!("basisread::Statement::ResizeScreen {{ width: {}, height: {} }}", expression_literal(width), expression_literal(height)),
        Statement::ListAdd { path, value } => format!("basisread::Statement::ListAdd {{ path: {}, value: {} }}", strings_literal(path), expression_literal(value)),
        Statement::ListRemove { path, value } => format!("basisread::Statement::ListRemove {{ path: {}, value: {} }}", strings_literal(path), expression_literal(value)),
        Statement::Shell(command) => format!("basisread::Statement::Shell({})", expression_literal(command)),
        Statement::StartShell(command) => format!("basisread::Statement::StartShell({})", expression_literal(command)),
        Statement::OpenFile(path) => format!("basisread::Statement::OpenFile({})", expression_literal(path)),
        Statement::Include(path) => format!("basisread::Statement::Include({})", expression_literal(path)),
        Statement::Stop => "basisread::Statement::Stop".to_string(),
        Statement::Skip => "basisread::Statement::Skip".to_string(),
        Statement::When { condition, body, otherwise } => format!(
            "basisread::Statement::When {{ condition: {}, body: {}, otherwise: {} }}",
            condition_literal(condition),
            statements_literal(body),
            otherwise.as_ref().map(|body| format!("Some({})", statements_literal(body))).unwrap_or_else(|| "None".to_string())
        ),
        Statement::Try { body, otherwise } => format!("basisread::Statement::Try {{ body: {}, otherwise: {} }}", statements_literal(body), statements_literal(otherwise)),
        Statement::Repeat { count, body } => format!("basisread::Statement::Repeat {{ count: {}, body: {} }}", expression_literal(count), statements_literal(body)),
        Statement::While { condition, body } => format!("basisread::Statement::While {{ condition: {}, body: {} }}", condition_literal(condition), statements_literal(body)),
        Statement::ForEach { name, iterable, body } => format!("basisread::Statement::ForEach {{ name: String::from({:?}), iterable: {}, body: {} }}", name, expression_literal(iterable), statements_literal(body)),
        Statement::Define { name, parameters, body } => format!("basisread::Statement::Define {{ name: String::from({:?}), parameters: {}, body: {} }}", name, strings_literal(parameters), statements_literal(body)),
        Statement::Return(expression) => format!("basisread::Statement::Return({})", expression_literal(expression)),
        Statement::Expression(expression) => format!("basisread::Statement::Expression({})", expression_literal(expression)),
    }
}

fn expressions_literal(expressions: &[Expression]) -> String {
    format!("vec![{}]", join(expressions.iter().map(expression_literal)))
}

fn strings_literal(strings: &[String]) -> String {
    format!("vec![{}]", join(strings.iter().map(|value| format!("String::from({value:?})"))))
}

fn object_expressions_literal(values: &[(String, Expression)]) -> String {
    format!("vec![{}]", join(values.iter().map(|(key, value)| format!("(String::from({key:?}), {})", expression_literal(value)))))
}

fn optional_expression_literal(expression: Option<&Expression>) -> String {
    expression.map(|expression| format!("Some(Box::new({}))", expression_literal(expression))).unwrap_or_else(|| "None".to_string())
}

fn expression_literal(expression: &Expression) -> String {
    match expression {
        Expression::Literal(value) => format!("basisread::Expression::Literal({})", value_literal(value)),
        Expression::Variable(name) => format!("basisread::Expression::Variable(String::from({name:?}))"),
        Expression::List(values) => format!("basisread::Expression::List({})", expressions_literal(values)),
        Expression::Object(values) => format!("basisread::Expression::Object({})", object_expressions_literal(values)),
        Expression::Field(value, field) => format!("basisread::Expression::Field(Box::new({}), String::from({field:?}))", expression_literal(value)),
        Expression::Ask(prompt) => format!("basisread::Expression::Ask(Box::new({}))", expression_literal(prompt)),
        Expression::AskKey => "basisread::Expression::AskKey".to_string(),
        Expression::ReadFile(value) => format!("basisread::Expression::ReadFile(Box::new({}))", expression_literal(value)),
        Expression::LoadFile(value) => format!("basisread::Expression::LoadFile(Box::new({}))", expression_literal(value)),
        Expression::Length(value) => format!("basisread::Expression::Length(Box::new({}))", expression_literal(value)),
        Expression::At(value, index) => format!("basisread::Expression::At(Box::new({}), Box::new({}))", expression_literal(value), expression_literal(index)),
        Expression::EnvironmentVariable(name) => format!("basisread::Expression::EnvironmentVariable(Box::new({}))", expression_literal(name)),
        Expression::CurrentFolder => "basisread::Expression::CurrentFolder".to_string(),
        Expression::FileExists(path) => format!("basisread::Expression::FileExists(Box::new({}))", expression_literal(path)),
        Expression::FolderExists(path) => format!("basisread::Expression::FolderExists(Box::new({}))", expression_literal(path)),
        Expression::ListFiles(path) => format!("basisread::Expression::ListFiles(Box::new({}))", expression_literal(path)),
        Expression::ListFolders(path) => format!("basisread::Expression::ListFolders(Box::new({}))", expression_literal(path)),
        Expression::ListApplications => "basisread::Expression::ListApplications".to_string(),
        Expression::TerminalWidth => "basisread::Expression::TerminalWidth".to_string(),
        Expression::TerminalHeight => "basisread::Expression::TerminalHeight".to_string(),
        Expression::ScreenWidth => "basisread::Expression::ScreenWidth".to_string(),
        Expression::ScreenHeight => "basisread::Expression::ScreenHeight".to_string(),
        Expression::Timer => "basisread::Expression::Timer".to_string(),
        Expression::RandomNumber { lower, upper, integer } => format!("basisread::Expression::RandomNumber {{ lower: {}, upper: {}, integer: {} }}", optional_expression_literal(lower.as_deref()), optional_expression_literal(upper.as_deref()), integer),
        Expression::RandomChoice(value) => format!("basisread::Expression::RandomChoice(Box::new({}))", expression_literal(value)),
        Expression::Minimum(left, right) => format!("basisread::Expression::Minimum(Box::new({}), Box::new({}))", expression_literal(left), expression_literal(right)),
        Expression::Maximum(left, right) => format!("basisread::Expression::Maximum(Box::new({}), Box::new({}))", expression_literal(left), expression_literal(right)),
        Expression::Clamp { value, lower, upper } => format!("basisread::Expression::Clamp {{ value: Box::new({}), lower: Box::new({}), upper: Box::new({}) }}", expression_literal(value), expression_literal(lower), expression_literal(upper)),
        Expression::Add(left, right) => format!("basisread::Expression::Add(Box::new({}), Box::new({}))", expression_literal(left), expression_literal(right)),
        Expression::Subtract(left, right) => format!("basisread::Expression::Subtract(Box::new({}), Box::new({}))", expression_literal(left), expression_literal(right)),
        Expression::Multiply(left, right) => format!("basisread::Expression::Multiply(Box::new({}), Box::new({}))", expression_literal(left), expression_literal(right)),
        Expression::Divide(left, right) => format!("basisread::Expression::Divide(Box::new({}), Box::new({}))", expression_literal(left), expression_literal(right)),
        Expression::Join(left, right) => format!("basisread::Expression::Join(Box::new({}), Box::new({}))", expression_literal(left), expression_literal(right)),
        Expression::Call { name, arguments } => format!("basisread::Expression::Call {{ name: String::from({name:?}), arguments: {} }}", expressions_literal(arguments)),
    }
}

fn value_literal(value: &Value) -> String {
    match value {
        Value::Text(value) => format!("basisread::Value::Text(String::from({value:?}))"),
        Value::Number(value) => format!("basisread::Value::Number({value:?})"),
        Value::Boolean(value) => format!("basisread::Value::Boolean({value})"),
        Value::List(values) => format!("basisread::Value::List({})", values_literal(values)),
        Value::Object(values) => format!("basisread::Value::Object(vec![{}].into_iter().collect::<std::collections::BTreeMap<String, basisread::Value>>())", join(values.iter().map(|(key, value)| format!("(String::from({key:?}), {})", value_literal(value))))),
        Value::Nothing => "basisread::Value::Nothing".to_string(),
    }
}

fn values_literal(values: &[Value]) -> String {
    format!("vec![{}]", join(values.iter().map(value_literal)))
}

fn condition_literal(condition: &Condition) -> String {
    match condition {
        Condition::Truthy(expression) => format!("basisread::Condition::Truthy({})", expression_literal(expression)),
        Condition::Not(condition) => format!("basisread::Condition::Not(Box::new({}))", condition_literal(condition)),
        Condition::And(left, right) => format!("basisread::Condition::And(Box::new({}), Box::new({}))", condition_literal(left), condition_literal(right)),
        Condition::Or(left, right) => format!("basisread::Condition::Or(Box::new({}), Box::new({}))", condition_literal(left), condition_literal(right)),
        Condition::Contains(left, right) => format!("basisread::Condition::Contains({}, {})", expression_literal(left), expression_literal(right)),
        Condition::StartsWith(left, right) => format!("basisread::Condition::StartsWith({}, {})", expression_literal(left), expression_literal(right)),
        Condition::EndsWith(left, right) => format!("basisread::Condition::EndsWith({}, {})", expression_literal(left), expression_literal(right)),
        Condition::Equals(left, right) => format!("basisread::Condition::Equals({}, {})", expression_literal(left), expression_literal(right)),
        Condition::NotEquals(left, right) => format!("basisread::Condition::NotEquals({}, {})", expression_literal(left), expression_literal(right)),
        Condition::GreaterThan(left, right) => format!("basisread::Condition::GreaterThan({}, {})", expression_literal(left), expression_literal(right)),
        Condition::LessThan(left, right) => format!("basisread::Condition::LessThan({}, {})", expression_literal(left), expression_literal(right)),
    }
}

fn join<I>(values: I) -> String
where
    I: IntoIterator<Item = String>,
{
    values.into_iter().collect::<Vec<_>>().join(", ")
}
