//! Numbers a composed text may hold (ADR-0020 and ADR-0038): the request's, those on the
//! screens of the task, and the results of calculations of known numbers that this module computes itself. A number is
//! read with its thousands separators (`1 686 222`, `1,686,222`, `1.686.222`), so it is known both as a whole
//! (`1686222`) and as its digit groups.

use std::collections::HashSet;

/// Most numbers kept for one task, so a page full of figures cannot grow the set without end.
const MAX_KNOWN: usize = 4_000;
/// Longest calculation the device evaluates.
const MAX_CALCULATION_CHARS: usize = 300;
/// Deepest nesting of parentheses in a calculation.
const MAX_DEPTH: usize = 8;
/// Largest whole number a calculation may use without having been seen, for averages and percentages (`/ 2`, `/ 100`).
const MAX_CONSTANT: u32 = 100;
/// Largest whole number a spreadsheet formula may use without having been seen: months, periods and percentages such
/// as `=PMT(6%/12,48,-12000)`. The app calculates a formula again from its
/// cells, so such a constant is no invented figure. The agent runtime allows the same.
const MAX_FORMULA_CONSTANT: u32 = 1_000;

/// The numbers a task may use.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KnownNumbers {
    whole: HashSet<String>,
    groups: HashSet<String>,
}

/// One number as written: its digits without separators, and its digit groups.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Written {
    whole: String,
    groups: Vec<String>,
}

/// Whether `character` may separate the digit groups of one number.
const fn is_separator(character: char) -> bool {
    matches!(character, ' ' | '\u{a0}' | '\u{202f}' | '.' | ',' | '\'')
}

/// The numbers written in `text`, each as its digits and its digit groups. A separator joins two groups only between
/// digits, so "2026. 10. 22" is three numbers only where a space follows the dot and a digit follows the space.
fn written_numbers(text: &str) -> Vec<Written> {
    let characters: Vec<char> = text.chars().collect();
    let mut numbers = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        if !characters[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let mut groups = vec![String::new()];
        while index < characters.len() {
            let character = characters[index];
            if character.is_ascii_digit() {
                if let Some(group) = groups.last_mut() {
                    group.push(character);
                }
                index += 1;
            } else if is_separator(character)
                && characters.get(index + 1).is_some_and(char::is_ascii_digit)
            {
                groups.push(String::new());
                index += 1;
            } else {
                break;
            }
        }
        numbers.push(Written {
            whole: groups.concat(),
            groups,
        });
    }
    numbers
}

impl KnownNumbers {
    /// The numbers of `text`.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut known = Self::default();
        known.add_text(text);
        known
    }

    /// Adds the numbers of `text`, up to the set's bound.
    pub fn add_text(&mut self, text: &str) {
        for number in written_numbers(text) {
            if self.whole.len() + self.groups.len() >= MAX_KNOWN {
                return;
            }
            self.groups.extend(number.groups);
            self.whole.insert(number.whole);
        }
    }

    /// Adds every number of `other`.
    pub fn extend(&mut self, other: &Self) {
        self.whole.extend(other.whole.iter().cloned());
        self.groups.extend(other.groups.iter().cloned());
    }

    /// How many numbers and digit groups are known.
    #[must_use]
    pub fn len(&self) -> usize {
        self.whole.len() + self.groups.len()
    }

    /// Whether nothing is known.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.whole.is_empty() && self.groups.is_empty()
    }

    /// Whether one written number is known: as a whole, or every one of its digit groups.
    fn knows(&self, number: &Written) -> bool {
        self.whole.contains(&number.whole)
            || number
                .groups
                .iter()
                .all(|group| self.groups.contains(group))
    }

    /// Whether every number written in `text` is known; in a spreadsheet formula (text that starts with `=`), a whole
    /// number up to `MAX_FORMULA_CONSTANT` (1,000) counts as known.
    #[must_use]
    pub fn allow(&self, text: &str) -> bool {
        let formula = text.trim_start().starts_with('=');
        let arguments = formula.then(|| formula_arguments(text));
        written_numbers(arguments.as_deref().unwrap_or(text))
            .iter()
            .all(|number| self.knows(number) || (formula && is_formula_constant(number)))
    }

    /// Adds the result of `calculation`, when it is a calculation of known numbers: as a whole number, and rounded to
    /// one and two decimals, the forms a text writes it in. Returns whether it was one.
    pub fn add_calculation(&mut self, calculation: &str) -> bool {
        let Some(result) = evaluate(calculation, self) else {
            return false;
        };
        for decimals in 0..=2 {
            let text = format!("{result:.decimals$}");
            for number in written_numbers(&text) {
                self.groups.extend(number.groups);
                self.whole.insert(number.whole);
            }
        }
        true
    }
}

/// The value of `calculation` (numbers, `+ - * / %` and parentheses), when every number in it is known or a whole
/// number up to `MAX_CONSTANT` (100) and the result is finite; `None` for anything else, such as a name, a call or a
/// division by zero.
#[must_use]
pub fn evaluate(calculation: &str, known: &KnownNumbers) -> Option<f64> {
    let operands_known = written_numbers(calculation)
        .iter()
        .all(|number| known.knows(number) || is_constant(number));
    if calculation.chars().count() > MAX_CALCULATION_CHARS || !operands_known {
        return None;
    }
    let tokens = tokenize(calculation)?;
    let mut parser = Parser { tokens, at: 0 };
    let value = parser.sum(0)?;
    (parser.at == parser.tokens.len() && value.is_finite()).then_some(value)
}

/// A formula with its argument separators apart from its numbers: `;` where the formula uses it, otherwise `,`, so
/// `=PMT(6%/12,48,-12000)` holds 12 and 48 rather than 12,48.
fn formula_arguments(text: &str) -> String {
    let separator = if text.contains(';') { ';' } else { ',' };
    text.replace(separator, "|")
}

/// Whether a written number is a whole number a spreadsheet formula may use without having seen it.
fn is_formula_constant(number: &Written) -> bool {
    number.groups.len() == 1
        && number
            .whole
            .parse::<u32>()
            .is_ok_and(|value| value <= MAX_FORMULA_CONSTANT)
}

/// Whether a written number is a small whole number a calculation may use without having seen it.
fn is_constant(number: &Written) -> bool {
    number.groups.len() == 1
        && number
            .whole
            .parse::<u32>()
            .is_ok_and(|value| value <= MAX_CONSTANT)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Number(f64),
    Operator(char),
    Open,
    Close,
}

/// The tokens of a calculation. A number may group its thousands with spaces and use a decimal point or comma.
fn tokenize(text: &str) -> Option<Vec<Token>> {
    let characters: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        match character {
            ' ' | '\u{a0}' | '\u{202f}' => index += 1,
            '+' | '-' | '*' | '/' | '%' | '×' | '÷' => {
                tokens.push(Token::Operator(match character {
                    '×' => '*',
                    '÷' => '/',
                    other => other,
                }));
                index += 1;
            }
            '(' => {
                tokens.push(Token::Open);
                index += 1;
            }
            ')' => {
                tokens.push(Token::Close);
                index += 1;
            }
            digit if digit.is_ascii_digit() => {
                let (value, next) = number_at(&characters, index)?;
                tokens.push(Token::Number(value));
                index = next;
            }
            _ => return None,
        }
    }
    Some(tokens)
}

/// The number that starts at `start`: digits with thousands spaces, and an optional decimal point or comma.
fn number_at(characters: &[char], start: usize) -> Option<(f64, usize)> {
    let mut digits = String::new();
    let mut index = start;
    while let Some(&character) = characters.get(index) {
        let next_is_digit = characters.get(index + 1).is_some_and(char::is_ascii_digit);
        if character.is_ascii_digit() {
            digits.push(character);
        } else if matches!(character, ' ' | '\u{a0}' | '\u{202f}') && next_is_digit {
            // A thousands space joins groups of one number.
        } else if matches!(character, '.' | ',') && next_is_digit && !digits.contains('.') {
            digits.push('.');
        } else {
            break;
        }
        index += 1;
    }
    digits.parse().ok().map(|value| (value, index))
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.at).copied()
    }

    fn sum(&mut self, depth: usize) -> Option<f64> {
        let mut value = self.product(depth)?;
        while let Some(Token::Operator(operator @ ('+' | '-'))) = self.peek() {
            self.at += 1;
            let right = self.product(depth)?;
            value = if operator == '+' {
                value + right
            } else {
                value - right
            };
        }
        Some(value)
    }

    fn product(&mut self, depth: usize) -> Option<f64> {
        let mut value = self.factor(depth)?;
        while let Some(Token::Operator(operator @ ('*' | '/' | '%'))) = self.peek() {
            self.at += 1;
            let right = self.factor(depth)?;
            value = match operator {
                '*' => value * right,
                '/' if right != 0.0 => value / right,
                '%' if right != 0.0 => value % right,
                _ => return None,
            };
        }
        Some(value)
    }

    fn factor(&mut self, depth: usize) -> Option<f64> {
        match self.peek()? {
            Token::Number(value) => {
                self.at += 1;
                Some(value)
            }
            Token::Operator('-') => {
                self.at += 1;
                self.factor(depth).map(|value| -value)
            }
            Token::Open if depth < MAX_DEPTH => {
                self.at += 1;
                let value = self.sum(depth + 1)?;
                (self.peek()? == Token::Close).then(|| self.at += 1)?;
                Some(value)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
