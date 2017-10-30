use std::error::Error;
use std::fmt;
use std::iter::Peekable;
use std::str::Chars;
use std::char;

#[derive(Debug)]
pub enum LexError {
    UnexpectedChar,
    MalformedEscapeSequence,
    MalformedNumber,
    MalformedChar,
}

impl Error for LexError {
    fn description(&self) -> &str {
        match *self {
            LexError::UnexpectedChar => "Unexpected character in input",
            LexError::MalformedEscapeSequence => "Unexpected values in escape sequence",
            LexError::MalformedNumber => "Unexpected characters in number",
            LexError::MalformedChar => "Char constant not a single character",
        }
    }

    fn cause(&self) -> Option<&Error> {
        None
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[derive(Debug)]
pub enum ParseError {
    BadInput,
    InputPastEndOfFile,
    UnknownOperator,
    MissingRParen,
    MissingLCurly,
    MissingRCurly,
    MissingRSquare,
    MalformedCallExpr,
    MalformedIndexExpr,
    VarExpectsIdentifier,
    FnMissingName,
    FnMissingParams,
}

impl Error for ParseError {
    fn description(&self) -> &str {
        match *self {
            ParseError::BadInput => "Unparseable characters in the input stream",
            ParseError::InputPastEndOfFile => "Input past end of file",
            ParseError::UnknownOperator => "Unknown operator",
            ParseError::MissingRParen => "Expected ')'",
            ParseError::MissingLCurly => "Expected '{'",
            ParseError::MissingRCurly => "Expected '}'",
            ParseError::MissingRSquare => "Expected ']'",
            ParseError::MalformedCallExpr => "Call contains bad expression",
            ParseError::MalformedIndexExpr => "Indexing expression missing correct index",
            ParseError::VarExpectsIdentifier => "'var' expects the name of a variable",
            ParseError::FnMissingName => "Function declaration is missing name",
            ParseError::FnMissingParams => "Function declaration is missing parameters",
        }
    }

    fn cause(&self) -> Option<&Error> { None }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Box<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    If(Box<Expr>, Box<Stmt>),
    IfElse(Box<Expr>, Box<Stmt>, Box<Stmt>),
    While(Box<Expr>, Box<Stmt>),
    Var(String, Option<Box<Expr>>),
    Block(Vec<Stmt>),
    Expr(Box<Expr>),
    Break,
    Return,
    ReturnWithVal(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    IntConst(i64),
    Identifier(String),
    CharConst(char),
    StringConst(String),
    FnCall(String, Vec<Expr>),
    Assignment(Box<Expr>, Box<Expr>),
    Dot(Box<Expr>, Box<Expr>),
    Index(String, Box<Expr>),
    Array(Vec<Expr>),
    True,
    False,
}

#[derive(Debug)]
pub enum Token {
    IntConst(i64),
    Identifier(String),
    CharConst(char),
    StringConst(String),
    LCurly,
    RCurly,
    LParen,
    RParen,
    LSquare,
    RSquare,
    Plus,
    Minus,
    Multiply,
    Divide,
    Semicolon,
    Colon,
    Comma,
    Period,
    Equals,
    True,
    False,
    Var,
    If,
    Else,
    While,
    LessThan,
    GreaterThan,
    Bang,
    LessThanEqual,
    GreaterThanEqual,
    EqualTo,
    NotEqualTo,
    Pipe,
    Or,
    Ampersand,
    And,
    Fn,
    Break,
    Return,
    LexErr(LexError),
}

pub struct TokenIterator<'a> {
    char_stream: Peekable<Chars<'a>>,
}

impl<'a> TokenIterator<'a> {
    pub fn parse_string_const(&mut self, enclosing_char: char) -> Result<String, LexError> {
        let mut result = Vec::new();
        let mut escape = false;

        while let Some(nxt) = self.char_stream.next() {
            match nxt {
                '\\' if !escape => escape = true,
                '\\' if escape => {
                    escape = false;
                    result.push('\\');
                }
                't' if escape => {
                    escape = false;
                    result.push('\t');
                }
                'n' if escape => {
                    escape = false;
                    result.push('\n');
                }
                'r' if escape => {
                    escape = false;
                    result.push('\r');
                }
                'x' if escape => {
                    escape = false;
                    let mut out_val: u32 = 0;
                    for _ in 0..2 {
                        if let Some(c) = self.char_stream.next() {
                            if let Some(d1) = c.to_digit(16) {
                                out_val *= 16;
                                out_val += d1;
                            } else {
                                return Err(LexError::MalformedEscapeSequence);
                            }
                        } else {
                            return Err(LexError::MalformedEscapeSequence);
                        }
                    }

                    if let Some(r) = char::from_u32(out_val) {
                        result.push(r);
                    } else {
                        return Err(LexError::MalformedEscapeSequence);
                    }
                }
                'u' if escape => {
                    escape = false;
                    let mut out_val: u32 = 0;
                    for _ in 0..4 {
                        if let Some(c) = self.char_stream.next() {
                            if let Some(d1) = c.to_digit(16) {
                                out_val *= 16;
                                out_val += d1;
                            } else {
                                return Err(LexError::MalformedEscapeSequence);
                            }
                        } else {
                            return Err(LexError::MalformedEscapeSequence);
                        }
                    }

                    if let Some(r) = char::from_u32(out_val) {
                        result.push(r);
                    } else {
                        return Err(LexError::MalformedEscapeSequence);
                    }
                }
                'U' if escape => {
                    escape = false;
                    let mut out_val: u32 = 0;
                    for _ in 0..8 {
                        if let Some(c) = self.char_stream.next() {
                            if let Some(d1) = c.to_digit(16) {
                                out_val *= 16;
                                out_val += d1;
                            } else {
                                return Err(LexError::MalformedEscapeSequence);
                            }
                        } else {
                            return Err(LexError::MalformedEscapeSequence);
                        }
                    }

                    if let Some(r) = char::from_u32(out_val) {
                        result.push(r);
                    } else {
                        return Err(LexError::MalformedEscapeSequence);
                    }
                }
                x if enclosing_char == x && escape => result.push(x),
                x if enclosing_char == x && !escape => break,
                _ if escape => return Err(LexError::MalformedEscapeSequence),
                _ => {
                    escape = false;
                    result.push(nxt);
                }
            }
        }

        let out: String = result.iter().cloned().collect();
        Ok(out)
    }
}

impl<'a> Iterator for TokenIterator<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(c) = self.char_stream.next() {
            match c {
                '0'...'9' => {
                    let mut result = Vec::new();
                    result.push(c);

                    while let Some(&nxt) = self.char_stream.peek() {
                        match nxt {
                            '0'...'9' => {
                                result.push(nxt);
                                self.char_stream.next();
                            }
                            _ => break,
                        }
                    }

                    let out: String = result.iter().cloned().collect();

                    if let Ok(val) = out.parse::<i64>() {
                        return Some(Token::IntConst(val));
                    }
                    return Some(Token::LexErr(LexError::MalformedNumber));
                }
                'A'...'Z' | 'a'...'z' | '_' => {
                    let mut result = Vec::new();
                    result.push(c);

                    while let Some(&nxt) = self.char_stream.peek() {
                        match nxt {
                            x if x.is_alphanumeric() || x == '_' => {
                                result.push(x);
                                self.char_stream.next();
                            }
                            _ => break,
                        }
                    }

                    let out: String = result.iter().cloned().collect();

                    match out.as_ref() {
                        "true" => return Some(Token::True),
                        "false" => return Some(Token::False),
                        "let" => return Some(Token::Var),
                        "if" => return Some(Token::If),
                        "else" => return Some(Token::Else),
                        "while" => return Some(Token::While),
                        "break" => return Some(Token::Break),
                        "return" => return Some(Token::Return),
                        "fn" => return Some(Token::Fn),
                        x => return Some(Token::Identifier(x.to_string()))
                    }
                }
                '"' => {
                    match self.parse_string_const('"') {
                        Ok(out) => return Some(Token::StringConst(out)),
                        Err(e) => return Some(Token::LexErr(e)),
                    }
                }
                '\'' => {
                    match self.parse_string_const('\'') {
                        Ok(result) => {
                            let mut chars = result.chars();

                            if let Some(out) = chars.next() {
                                println!("result: {}", result);
                                if chars.count() != 0 {
                                    return Some(Token::LexErr(LexError::MalformedChar));
                                }
                                return Some(Token::CharConst(out));
                            } else {
                                return Some(Token::LexErr(LexError::MalformedChar));
                            }
                        }
                        Err(e) => return Some(Token::LexErr(e)),
                    }
                }
                '{' => return Some(Token::LCurly),
                '}' => return Some(Token::RCurly),
                '(' => return Some(Token::LParen),
                ')' => return Some(Token::RParen),
                '[' => return Some(Token::LSquare),
                ']' => return Some(Token::RSquare),
                '+' => return Some(Token::Plus),
                '-' => return Some(Token::Minus),
                '*' => return Some(Token::Multiply),
                '/' => {
                    if let Some(&comment) = self.char_stream.peek() {
                        match comment {
                            '/' => {
                                self.char_stream.next();
                                while let Some(&comment_next) = self.char_stream.peek() {
                                    match comment_next {
                                        '\n' => break,
                                        _ => { self.char_stream.next(); }
                                    }
                                }
                            }
                            '*' => {
                                self.char_stream.next();
                                while let Some(&comment_next) = self.char_stream.peek() {
                                    match comment_next {
                                        '*' => {
                                            self.char_stream.next();
                                            match self.char_stream.peek() {
                                                Some(&'/') => { self.char_stream.next(); break; },
                                                _ => { self.char_stream.next(); }
                                            }
                                        }
                                        _ => { self.char_stream.next(); }
                                    }
                                }
                            }
                            _ => return Some(Token::Divide)
                        }
                    }
                },
                ';' => return Some(Token::Semicolon),
                ':' => return Some(Token::Colon),
                ',' => return Some(Token::Comma),
                '.' => return Some(Token::Period),
                '=' => {
                    match self.char_stream.peek() {
                        Some(&'=') => {
                            self.char_stream.next();
                            return Some(Token::EqualTo);
                        }
                        _ => return Some(Token::Equals),
                    }
                }
                '<' => {
                    match self.char_stream.peek() {
                        Some(&'=') => {
                            self.char_stream.next();
                            return Some(Token::LessThanEqual);
                        }
                        _ => return Some(Token::LessThan),
                    }
                }
                '>' => {
                    match self.char_stream.peek() {
                        Some(&'=') => {
                            self.char_stream.next();
                            return Some(Token::GreaterThanEqual);
                        }
                        _ => return Some(Token::GreaterThan),
                    }
                }
                '!' => {
                    match self.char_stream.peek() {
                        Some(&'=') => {
                            self.char_stream.next();
                            return Some(Token::NotEqualTo);
                        }
                        _ => return Some(Token::Bang),
                    }
                }
                '|' => {
                    match self.char_stream.peek() {
                        Some(&'|') => {
                            self.char_stream.next();
                            return Some(Token::Or);
                        }
                        _ => return Some(Token::Pipe),
                    }
                }
                '&' => {
                    match self.char_stream.peek() {
                        Some(&'&') => {
                            self.char_stream.next();
                            return Some(Token::And);
                        }
                        _ => return Some(Token::Ampersand),
                    }
                }
                _x if _x.is_whitespace() => (),
                _ => return Some(Token::LexErr(LexError::UnexpectedChar)),
            }
        }

        None
    }
}

pub fn lex(input: &str) -> TokenIterator {
    TokenIterator { char_stream: input.chars().peekable() }
}

fn get_precedence(token: &Token) -> i32 {
    match *token {
        Token::Equals => 10,
        Token::Or => 11,
        Token::And => 12,
        Token::LessThan
        | Token::LessThanEqual
        | Token::GreaterThan
        | Token::GreaterThanEqual
        | Token::EqualTo
        | Token::NotEqualTo => 15,
        Token::Plus
        | Token::Minus => 20,
        Token::Divide
        | Token::Multiply => 40,
        Token::Period => 100,
        _ => -1,
    }
}

fn parse_paren_expr<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Expr, ParseError> {
    let expr = try!(parse_expr(input));

    match input.next() {
        Some(Token::RParen) => Ok(expr),
        _ => Err(ParseError::MissingRParen),
    }
}

fn parse_call_expr<'a>(id: String,
                       input: &mut Peekable<TokenIterator<'a>>)
                       -> Result<Expr, ParseError> {
    let mut args = Vec::new();

    if let Some(&Token::RParen) = input.peek() {
        input.next();
        return Ok(Expr::FnCall(id, args));
    }

    loop {
        if let Ok(arg) = parse_expr(input) {
            args.push(arg);
        } else {
            return Err(ParseError::MalformedCallExpr);
        }

        match input.peek() {
            Some(&Token::RParen) => {
                input.next();
                return Ok(Expr::FnCall(id, args));
            }
            Some(&Token::Comma) => (),
            _ => return Err(ParseError::MalformedCallExpr),
        }

        input.next();
    }
}

fn parse_index_expr<'a>(id: String,
                        input: &mut Peekable<TokenIterator<'a>>)
                        -> Result<Expr, ParseError> {
    if let Ok(idx) = parse_expr(input) {
        match input.peek() {
            Some(&Token::RSquare) => {
                input.next();
                return Ok(Expr::Index(id, Box::new(idx)));
            }
            _ => return Err(ParseError::MalformedIndexExpr),
        }
    } else {
        return Err(ParseError::MalformedIndexExpr);
    }
}

fn parse_ident_expr<'a>(id: String,
                        input: &mut Peekable<TokenIterator<'a>>)
                        -> Result<Expr, ParseError> {
    match input.peek() {
        Some(&Token::LParen) => {
            input.next();
            parse_call_expr(id, input)
        }
        Some(&Token::LSquare) => {
            input.next();
            parse_index_expr(id, input)
        }
        _ => Ok(Expr::Identifier(id)),
    }
}

fn parse_array_expr<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Expr, ParseError> {
    let mut arr = Vec::new();

    let skip_contents = match input.peek() {
        Some(&Token::RSquare) => true,
        _ => false,
    };

    if !skip_contents {
        while let Some(_) = input.peek() {
            arr.push(try!(parse_expr(input)));
            if let Some(&Token::Comma) = input.peek() {
                input.next();
            }

            if let Some(&Token::RSquare) = input.peek() { break }
        }
    }

    match input.peek() {
        Some(&Token::RSquare) => {
            input.next();
            Ok(Expr::Array(arr))
        }
        _ => Err(ParseError::MissingRSquare),
    }

}

fn parse_primary<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Expr, ParseError> {
    if let Some(token) = input.next() {
        match token {
            Token::IntConst(ref x) => Ok(Expr::IntConst(*x)),
            Token::StringConst(ref s) => Ok(Expr::StringConst(s.clone())),
            Token::CharConst(ref c) => Ok(Expr::CharConst(*c)),
            Token::Identifier(ref s) => parse_ident_expr(s.clone(), input),
            Token::LParen => parse_paren_expr(input),
            Token::LSquare => parse_array_expr(input),
            Token::True => Ok(Expr::True),
            Token::False => Ok(Expr::False),
            Token::LexErr(le) => {
                println!("Error: {}", le);
                Err(ParseError::BadInput)
            }
            _ => {
                println!("Can't parse: {:?}", token);
                Err(ParseError::BadInput)
            }
        }
    } else {
        Err(ParseError::InputPastEndOfFile)
    }
}

fn parse_binop<'a>(input: &mut Peekable<TokenIterator<'a>>,
                   prec: i32,
                   lhs: Expr)
                   -> Result<Expr, ParseError> {
    let mut lhs_curr = lhs;

    loop {
        let mut curr_prec = -1;

        if let Some(curr_op) = input.peek() {
            curr_prec = get_precedence(curr_op);
        }

        if curr_prec < prec {
            return Ok(lhs_curr);
        }

        if let Some(op_token) = input.next() {
            let mut rhs = try!(parse_primary(input));

            let mut next_prec = -1;

            if let Some(next_op) = input.peek() {
                next_prec = get_precedence(next_op);
            }

            if curr_prec < next_prec {
                rhs = try!(parse_binop(input, curr_prec + 1, rhs));
            } else if curr_prec >= 100 {
                // Always bind right to left for precedence over 100
                rhs = try!(parse_binop(input, curr_prec, rhs));
            }

            lhs_curr = match op_token {
                Token::Plus => Expr::FnCall("+".to_string(), vec![lhs_curr, rhs]),
                Token::Minus => Expr::FnCall("-".to_string(), vec![lhs_curr, rhs]),
                Token::Multiply => Expr::FnCall("*".to_string(), vec![lhs_curr, rhs]),
                Token::Divide => Expr::FnCall("/".to_string(), vec![lhs_curr, rhs]),
                Token::Equals => Expr::Assignment(Box::new(lhs_curr), Box::new(rhs)),
                Token::Period => Expr::Dot(Box::new(lhs_curr), Box::new(rhs)),
                Token::EqualTo => Expr::FnCall("==".to_string(), vec![lhs_curr, rhs]),
                Token::NotEqualTo => Expr::FnCall("!=".to_string(), vec![lhs_curr, rhs]),
                Token::LessThan => Expr::FnCall("<".to_string(), vec![lhs_curr, rhs]),
                Token::LessThanEqual => {
                    Expr::FnCall("<=".to_string(), vec![lhs_curr, rhs])
                }
                Token::GreaterThan => Expr::FnCall(">".to_string(), vec![lhs_curr, rhs]),
                Token::GreaterThanEqual => {
                    Expr::FnCall(">=".to_string(), vec![lhs_curr, rhs])
                }
                Token::Or => Expr::FnCall("||".to_string(), vec![lhs_curr, rhs]),
                Token::And => Expr::FnCall("&&".to_string(), vec![lhs_curr, rhs]),
                _ => return Err(ParseError::UnknownOperator),
            };
        }
    }
}

fn parse_expr<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Expr, ParseError> {
    let lhs = try!(parse_primary(input));

    parse_binop(input, 0, lhs)
}

fn parse_if<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Stmt, ParseError> {
    input.next();

    let guard = try!(parse_expr(input));
    let body = try!(parse_block(input));

    match input.peek() {
        Some(&Token::Else) => {
            input.next();
            let else_body = try!(parse_block(input));
            Ok(Stmt::IfElse(Box::new(guard), Box::new(body), Box::new(else_body)))
        }
        _ => Ok(Stmt::If(Box::new(guard), Box::new(body))),
    }
}

fn parse_while<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Stmt, ParseError> {
    input.next();

    let guard = try!(parse_expr(input));
    let body = try!(parse_block(input));

    Ok(Stmt::While(Box::new(guard), Box::new(body)))
}

fn parse_var<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Stmt, ParseError> {
    input.next();

    let name = match input.next() {
        Some(Token::Identifier(ref s)) => s.clone(),
        _ => return Err(ParseError::VarExpectsIdentifier),
    };

    match input.peek() {
        Some(&Token::Equals) => {
            input.next();
            let initializer = try!(parse_expr(input));
            Ok(Stmt::Var(name, Some(Box::new(initializer))))
        }
        _ => Ok(Stmt::Var(name, None)),
    }
}

fn parse_block<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Stmt, ParseError> {
    match input.peek() {
        Some(&Token::LCurly) => (),
        _ => return Err(ParseError::MissingLCurly),
    }

    input.next();

    let mut stmts = Vec::new();

    let skip_body = match input.peek() {
        Some(&Token::RCurly) => true,
        _ => false,
    };

    if !skip_body {
        while let Some(_) = input.peek() {
            stmts.push(try!(parse_stmt(input)));

            if let Some(&Token::Semicolon) = input.peek() {
                input.next();
            }

            if let Some(&Token::RCurly) = input.peek() { break }
        }
    }

    match input.peek() {
        Some(&Token::RCurly) => {
            input.next();
            Ok(Stmt::Block(stmts))
        }
        _ => Err(ParseError::MissingRCurly),
    }
}

fn parse_expr_stmt<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Stmt, ParseError> {
    let expr = try!(parse_expr(input));
    Ok(Stmt::Expr(Box::new(expr)))
}

fn parse_stmt<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<Stmt, ParseError> {
    match input.peek() {
        Some(&Token::If) => parse_if(input),
        Some(&Token::While) => parse_while(input),
        Some(&Token::Break) => {
            input.next();
            Ok(Stmt::Break)
        }
        Some(&Token::Return) => {
            input.next();
            match input.peek() {
                Some(&Token::Semicolon) => Ok(Stmt::Return),
                _ => {
                    let ret = try!(parse_expr(input));
                    Ok(Stmt::ReturnWithVal(Box::new(ret)))
                }
            }
        }
        Some(&Token::LCurly) => parse_block(input),
        Some(&Token::Var) => parse_var(input),
        _ => parse_expr_stmt(input),
    }
}

fn parse_fn<'a>(input: &mut Peekable<TokenIterator<'a>>) -> Result<FnDef, ParseError> {
    input.next();

    let name = match input.next() {
        Some(Token::Identifier(ref s)) => s.clone(),
        _ => return Err(ParseError::FnMissingName),
    };

    match input.peek() {
        Some(&Token::LParen) => {
            input.next();
        }
        _ => return Err(ParseError::FnMissingParams),
    }

    let mut params = Vec::new();

    let skip_params = match input.peek() {
        Some(&Token::RParen) => {
            input.next();
            true
        }
        _ => false,
    };

    if !skip_params {
        loop {
            match input.next() {
                Some(Token::RParen) => break,
                Some(Token::Comma) => (),
                Some(Token::Identifier(ref s)) => {
                    params.push(s.clone());
                }
                _ => return Err(ParseError::MalformedCallExpr),
            }
        }
    }

    let body = try!(parse_block(input));

    Ok(FnDef {
        name: name,
        params: params,
        body: Box::new(body),
    })
}

fn parse_top_level<'a>(input: &mut Peekable<TokenIterator<'a>>)
                       -> Result<(Vec<Stmt>, Vec<FnDef>), ParseError> {
    let mut stmts = Vec::new();
    let mut fndefs = Vec::new();

    while let Some(_) = input.peek() {
        match input.peek() {
            Some(&Token::Fn) => fndefs.push(try!(parse_fn(input))),
            _ => stmts.push(try!(parse_stmt(input))),
        }

        if let Some(&Token::Semicolon) = input.peek() {
            input.next();
        }
    }

    Ok((stmts, fndefs))
}

pub fn parse<'a>(input: &mut Peekable<TokenIterator<'a>>)
                 -> Result<(Vec<Stmt>, Vec<FnDef>), ParseError> {
    parse_top_level(input)
}
