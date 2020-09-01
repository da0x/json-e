#![allow(unused_variables)]
#![allow(dead_code)]
use crate::errors::Error;
use crate::prattparser::{Context, PrattParser};
use crate::tokenizer::Token;
use json::number::Number;
use json::JsonValue;
use std::collections::HashMap;

pub(crate) struct Interpreter {
    parser: PrattParser<'static, JsonValue, HashMap<String, JsonValue>>,
}

impl Interpreter {
    pub(crate) fn new() -> Interpreter {
        Interpreter {
            // this would only fail if the parser itself were malformed
            parser: create_interpreter().expect("Interpreter initialization failed"),
        }
    }

    /// Parse a string with the given context.
    pub(crate) fn parse(
        &self,
        source: &str,
        context: HashMap<String, JsonValue>,
    ) -> Result<JsonValue, Error> {
        self.parser.parse(source, context, 0)
    }
}

fn parse_list(
    context: &mut Context<JsonValue, HashMap<String, JsonValue>>,
    separator: &str,
    terminator: &str,
) -> Result<JsonValue, Error> {
    let mut list: Vec<JsonValue> = vec![];

    if context.attempt(|t| t == terminator)? == None {
        loop {
            list.push(context.parse(None)?);
            if context.attempt(|t| t == separator)? == None {
                break;
            }
        }
        context.require(|t| t == terminator)?;
    }

    Ok(JsonValue::Array(list))
}

fn parse_object(
    context: &mut Context<JsonValue, HashMap<String, JsonValue>>,
) -> Result<JsonValue, Error> {
    let mut obj = HashMap::new();

    if context.attempt(|t| t == "}")? == None {
        loop {
            let mut k = context.require(|t| t == "identifier" || t == "string")?;

            if k.token_type == "string" {
                k.value = &k.value[1..k.value.len() - 1];
            }
            context.require(|t| t == ":")?;
            let v = context.parse(None)?;
            obj.insert(k.value.to_string(), v);

            if context.attempt(|t| t == ",")? == None {
                break;
            }
        }
        context.require(|t| t == "}")?;
    }

    Ok(JsonValue::from(obj))
}

fn parse_string(string: &str) -> Result<JsonValue, Error> {
    Ok(JsonValue::String(string[1..string.len() - 1].into()))
}

fn create_interpreter() -> Result<PrattParser<'static, JsonValue, HashMap<String, JsonValue>>, Error>
{
    let mut patterns = HashMap::new();
    patterns.insert("number", "[0-9]+(?:\\.[0-9]+)?");
    patterns.insert("identifier", "[a-zA-Z_][a-zA-Z_0-9]*");
    patterns.insert("string", "\'[^\']*\'|\"[^\"]*\"");
    // avoid matching these as prefixes of identifiers e.g., `insinutations`
    patterns.insert("true", "true(?![a-zA-Z_0-9])");
    patterns.insert("false", "false(?![a-zA-Z_0-9])");
    patterns.insert("in", "in(?![a-zA-Z_0-9])");
    patterns.insert("null", "null(?![a-zA-Z_0-9])");

    let token_types = vec![
        "**",
        "+",
        "-",
        "*",
        "/",
        "[",
        "]",
        ".",
        "(",
        ")",
        "{",
        "}",
        ":",
        ",",
        ">=",
        "<=",
        "<",
        ">",
        "==",
        "!=",
        "!",
        "&&",
        "||",
        "true",
        "false",
        "in",
        "null",
        "number",
        "identifier",
        "string",
    ];

    let precedence = vec![
        vec!["||"],
        vec!["&&"],
        vec!["in"],
        vec!["==", "!="],
        vec![">=", "<=", "<", ">"],
        vec!["+", "-"],
        vec!["*", "/"],
        vec!["**-right-associative"],
        vec!["**"],
        vec!["[", "."],
        vec!["("],
        vec!["unary"],
    ];

    let mut prefix_rules: HashMap<
        &str,
        fn(&Token, &mut Context<JsonValue, HashMap<String, JsonValue>>) -> Result<JsonValue, Error>,
    > = HashMap::new();

    prefix_rules.insert("number", |token, _context| {
        let n: Number = token.value.parse::<f64>()?.into();
        Ok(JsonValue::Number(n))
    });

    prefix_rules.insert("!", |_token, context| {
        let operand = context.parse(Some("unary"))?;
        match operand {
            JsonValue::Null => Ok(JsonValue::Boolean(true)),
            JsonValue::Short(o) => Ok(JsonValue::Boolean(o.len() == 0)),
            JsonValue::String(o) => Ok(JsonValue::Boolean(o.len() == 0)),
            JsonValue::Number(o) => Ok(JsonValue::Boolean(o == 0)),
            JsonValue::Array(o) => Ok(JsonValue::Boolean(o.len() == 0)),
            JsonValue::Object(o) => Ok(JsonValue::Boolean(o.is_empty())),
            JsonValue::Boolean(o) => Ok(JsonValue::Boolean(!o)),
        }
    });

    prefix_rules.insert("-", |_token, context| {
        let v = context.parse(Some("unary"))?;
        if let Some(n) = v.as_number() {
            return Ok(JsonValue::Number(-n));
        } else {
            return Err(Error::InterpreterError(
                "This operator expects a number".to_string(),
            ));
        }
    });

    prefix_rules.insert("+", |_token, context| {
        let v = context.parse(Some("unary"))?;
        if let Some(n) = v.as_number() {
            return Ok(JsonValue::Number(n));
        } else {
            return Err(Error::InterpreterError(
                "This operator expects a number".to_string(),
            ));
        }
    });

    prefix_rules.insert("identifier", |token, context| {
        if let Some(v) = context.context.get(token.value) {
            return Ok(v.clone());
        }
        return Err(Error::InterpreterError(format!(
            "unknown context value {}",
            token.value
        )));
    });

    prefix_rules.insert("null", |_token, _context| Ok(JsonValue::Null));

    prefix_rules.insert("[", |_token, context| parse_list(context, ",", "]"));

    prefix_rules.insert("(", |_token, context| {
        let expression = context.parse(None)?;

        context.require(|t| t == ")")?;

        Ok(expression)
    });

    prefix_rules.insert("{", |_token, context| parse_object(context));

    prefix_rules.insert("string", |token, _context| {
        Ok(JsonValue::String(
            token.value[1..token.value.len() - 1].into(),
        ))
    });

    prefix_rules.insert("true", |_token, _context| Ok(JsonValue::Boolean(true)));

    prefix_rules.insert("false", |_token, _context| Ok(JsonValue::Boolean(false)));

    let mut infix_rules: HashMap<
        &str,
        fn(
            &JsonValue,
            &Token,
            &mut Context<JsonValue, HashMap<String, JsonValue>>,
        ) -> Result<JsonValue, Error>,
    > = HashMap::new();

    infix_rules.insert("+", |left, token, context| {
        let right = context.parse(Some("+"))?;

        match (&left, &right) {
            (&JsonValue::Number(_), &JsonValue::Number(_)) => {
                let sum = left.as_f64().unwrap() + right.as_f64().unwrap();
                Ok(JsonValue::Number(sum.into()))
            }
            (&JsonValue::String(_), &JsonValue::String(_)) => {
                let sum = [left.as_str().unwrap(), right.as_str().unwrap()].concat();
                Ok(JsonValue::String(sum))
            }
            (_, _) => Err(Error::InterpreterError(
                "infix: +', 'number/string + number/string".to_string(),
            )),
        }
    });

    infix_rules.insert("*", |left, token, context| {
        let right = context.parse(Some("*"))?;

        match (&left, &right) {
            (&JsonValue::Number(_), &JsonValue::Number(_)) => {
                let product = left.as_f64().unwrap() * right.as_f64().unwrap();
                Ok(JsonValue::Number(product.into()))
            }
            (_, _) => Err(Error::InterpreterError(
                "infix: *', 'number + number".to_string(),
            )),
        }
    });

    infix_rules.insert("**", |left, token, context| {
        let right = context.parse(Some("**-right-associative"))?;

        match (&left, &right) {
            (&JsonValue::Number(_), &JsonValue::Number(_)) => {
                let result = left.as_f64().unwrap().powf(right.as_f64().unwrap());
                Ok(JsonValue::Number(result.into()))
            }
            (_, _) => Err(Error::InterpreterError(
                "infix: **', 'number + number".to_string(),
            )),
        }
    });

    // todo infix [
    #[allow(unused_assignments)] // temporary
    infix_rules.insert("[", |left, token, context| {
        let mut a: i64;
        let b: i64;
        let mut is_interval = false;

        // Successful cases this function handles:
        //  [1,2][1] - integer index of array [DONE]
        //  [1,2][1:2] - interval of array
        //  "abc"[1] - integer index of string
        //  "abc"[1:2] - interval of string
        //  {bar: 10}['bar'] - object property access

        if context.attempt(|t| t == ":")?.is_some() {
            a = 0;
            is_interval = true;
        } else {
            let parsed = context.parse(None)?;
            print!("parsed is {}\n", parsed);
            if let JsonValue::Number(n) = parsed {
                let (_, _, exponent) = n.as_parts();
                if exponent < 0 || n.is_nan() {
                    return Err(Error::InterpreterError(
                        "should only use integers to access arrays or strings".to_string(),
                    ));
                }
                a = n.as_fixed_point_i64(0).unwrap();
                if context.attempt(|t| t == ":")?.is_some() {
                    is_interval = true;
                }
            } else {
                return Err(Error::InterpreterError(
                    "left part of slice operator is not a number".to_string(),
                ));
            }
        }

        if is_interval && !context.attempt(|t| t == "]")?.is_some() {
            let parsed = context.parse(None)?;
            if let JsonValue::Number(n) = parsed {
                let (_, _, exponent) = n.as_parts();
                if exponent < 0 || n.is_nan() {
                    return Err(Error::InterpreterError(
                        "cannot perform interval access with non-integers".to_string(),
                    ));
                }
                b = n.as_fixed_point_i64(0).unwrap();
            } else {
                return Err(Error::InterpreterError(
                    "right part of slice operator is not a number".to_string(),
                ));
            }

            context.require(|t| t == "]")?;
        }

        if is_interval {
            unreachable!()
        } else {
            match left {
                JsonValue::Array(ref arr) => {
                    if a < 0 {
                        a = a + (arr.len() as i64);
                    }

                    if a >= (arr.len() as i64) || a < 0 {
                        print!("index is {}\n", a);
                        return Err(Error::InterpreterError("index out of bounds".to_string()));
                    }

                    return Ok(arr[a as usize].clone());
                }

                _ => unreachable!(),
            }
        }
    });

    // todo infix .
    // todo infix (
    // todo infix ==
    // todo infix >= <= > < == !=
    // todo infix ||
    // todo infix &&
    // todo infix in

    PrattParser::new(
        "\\s+",
        patterns,
        token_types,
        precedence,
        prefix_rules,
        infix_rules,
    )
}

#[cfg(test)]
mod tests {
    use crate::errors::Error;
    use crate::interpreter::create_interpreter;
    use json::{object::Object, JsonValue};
    use std::collections::HashMap;

    #[test]
    fn parse_number_expression() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(
            interpreter.parse("23.67", HashMap::new(), 0).unwrap(),
            23.67
        );
    }

    #[test]
    fn parse_boolean_negation() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("!true", HashMap::new(), 0).unwrap(),
            false
        );
    }

    #[test]
    fn parse_minus_expression_negative_number() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(interpreter.parse("-7", HashMap::new(), 0).unwrap(), -7);
    }

    #[test]
    fn parse_minus_expression_double_negative() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(interpreter.parse("--7", HashMap::new(), 0).unwrap(), 7);
    }

    #[test]
    fn parse_minus_expression_plus() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(interpreter.parse("-+10", HashMap::new(), 0).unwrap(), -10);
    }

    #[test]
    fn parse_minus_expression_zero() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(interpreter.parse("-0", HashMap::new(), 0).unwrap(), 0);
    }

    #[test]
    fn parse_plus_expression_positive_number() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(interpreter.parse("+5", HashMap::new(), 0).unwrap(), 5);
    }

    #[test]
    fn parse_plus_expression_zero() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(interpreter.parse("+0", HashMap::new(), 0).unwrap(), 0);
    }

    #[test]
    fn parse_plus_expression_minus() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(interpreter.parse("+-10", HashMap::new(), 0).unwrap(), -10);
    }

    #[test]
    fn parse_boolean_true() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(interpreter.parse("true", HashMap::new(), 0).unwrap(), true);
    }

    #[test]
    fn parse_boolean_false() {
        let interpreter = create_interpreter().unwrap();

        assert_eq!(
            interpreter.parse("false", HashMap::new(), 0).unwrap(),
            false
        );
    }

    #[test]
    fn parse_identifier_positive() {
        let interpreter = create_interpreter().unwrap();
        let mut context = HashMap::new();
        context.insert("x".to_string(), JsonValue::Number(10.into()));

        assert_eq!(interpreter.parse("x", context, 0).unwrap(), 10);
    }

    #[test]
    fn parse_identifier_negative_non_existing_variable() {
        let interpreter = create_interpreter().unwrap();
        let mut context = HashMap::new();
        context.insert("x".to_string(), JsonValue::Number(10.into()));

        assert_eq!(
            interpreter.parse("y", context, 0).err(),
            Some(Error::InterpreterError(
                "unknown context value y".to_string()
            ))
        );
    }

    #[test]
    fn parse_array_positive() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("[1, 2]", HashMap::new(), 0).unwrap(),
            JsonValue::Array(vec![1.into(), 2.into()]),
        );
    }

    #[test]
    fn parse_array_empty() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("[]", HashMap::new(), 0).unwrap(),
            JsonValue::Array(vec![]),
        );
    }

    #[test]
    fn parse_parens_negative() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("(true]", HashMap::new(), 0).err(),
            Some(Error::SyntaxError("Unexpected token error".to_string())),
        );
    }

    #[test]
    fn parse_parens() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("2*(3+4)", HashMap::new(), 0).unwrap(),
            JsonValue::Number(14.into()),
        );
    }

    #[test]
    fn parse_curly_braces() {
        let interpreter = create_interpreter().unwrap();
        let mut obj = HashMap::new();
        obj.insert("a".to_string(), JsonValue::Number(10.into()));
        assert_eq!(
            interpreter.parse("{a: 10}", HashMap::new(), 0).unwrap(),
            JsonValue::from(obj),
        );
    }

    #[test]
    fn parse_curly_braces_negative_semicolon() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("{b: 20; a: 10}", HashMap::new(), 0).err(),
            Some(Error::SyntaxError(
                "unexpected EOF for {b: 20; a: 10} at ; a: 10}".to_string()
            )),
        );
    }

    #[test]
    fn parse_curly_braces_two_keys() {
        let interpreter = create_interpreter().unwrap();
        let mut obj = HashMap::new();
        obj.insert("a".to_string(), JsonValue::Number(10.into()));
        obj.insert("b".to_string(), JsonValue::Number(20.into()));
        assert_eq!(
            interpreter
                .parse("{b: 20, a: 10}", HashMap::new(), 0)
                .unwrap(),
            JsonValue::from(obj),
        );
    }

    #[test]
    fn parse_curly_braces_empty() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("{}", HashMap::new(), 0).unwrap(),
            JsonValue::Object(Object::new()),
        );
    }

    #[test]
    fn parse_curly_braces_string() {
        let interpreter = create_interpreter().unwrap();
        let mut obj = HashMap::new();
        obj.insert("abc def".to_string(), JsonValue::Number(10.into()));
        assert_eq!(
            interpreter
                .parse("{\"abc def\": 10}", HashMap::new(), 0)
                .unwrap(),
            JsonValue::from(obj),
        );
    }

    #[test]
    fn parse_string() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("\"banana\"", HashMap::new(), 0).unwrap(),
            JsonValue::String("banana".to_string()),
        );
    }

    #[test]
    fn parse_string_empty() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("\"\"", HashMap::new(), 0).unwrap(),
            JsonValue::String("".to_string()),
        );
    }

    #[test]
    fn parse_string_addition() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter
                .parse("\"banana\" + \"chocolate\"", HashMap::new(), 0)
                .unwrap(),
            JsonValue::String("bananachocolate".to_string()),
        );
    }

    #[test]
    fn parse_exponentiation() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("2**3", HashMap::new(), 0).unwrap(),
            JsonValue::Number(8.into()),
        );
    }

    #[test]
    fn parse_exponentiation_zero_base() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("2**0", HashMap::new(), 0).unwrap(),
            JsonValue::Number(1.into()),
        );
    }

    #[test]
    fn parse_array_of_integers() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("[1,2][1]", HashMap::new(), 0).unwrap(),
            JsonValue::Number(2.into()),
        );
    }

    #[test]
    fn parse_array_negative_index() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("[1,2][-1]", HashMap::new(), 0).unwrap(),
            JsonValue::Number(2.into()),
        );
    }

    #[test]
    fn parse_array_noninteger_index() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("[1,2][0.7]", HashMap::new(), 0).err(),
            Some(Error::InterpreterError(
                "should only use integers to access arrays or strings".to_string()
            ))
        );
    }

    #[test]
    fn parse_array_string_index() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("[1,2]['foo']", HashMap::new(), 0).err(),
            Some(Error::InterpreterError(
                // TODO: this is not the same message as JS
                "left part of slice operator is not a number".to_string()
            ))
        );
    }

    #[test]
    fn parse_array_positive_index_overflow() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("[1,2][9]", HashMap::new(), 0).err(),
            Some(Error::InterpreterError("index out of bounds".to_string()))
        );
    }

    #[test]
    fn parse_array_negative_index_overflow() {
        let interpreter = create_interpreter().unwrap();
        assert_eq!(
            interpreter.parse("[1,2][-9]", HashMap::new(), 0).err(),
            Some(Error::InterpreterError("index out of bounds".to_string()))
        );
    }
}
