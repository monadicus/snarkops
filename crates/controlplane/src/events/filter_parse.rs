use std::{fmt::Display, str::FromStr, sync::Arc};

use snops_common::node_targets::{NodeTarget, NodeTargets};

use super::EventFilter;
use crate::events::EventKindFilter;

/*

Example EventFilter string representation:

unfiltered
any-of(agent-connected, agent-disconnected)
all-of(not(agent-is(foo-bar)), env-is(default))
node-key-is(client/foo)
node-target-is(client/test-*@*)
node-target-is(client/any)
not(unfiltered)

*/

#[derive(Debug, Copy, Clone)]
enum Token<'a> {
    OpenParen,
    CloseParen,
    Comma,
    Whitespace,
    Text(&'a str),
}

impl<'a> Token<'a> {
    fn label(self) -> &'static str {
        match self {
            Token::OpenParen => "open paren",
            Token::CloseParen => "close paren",
            Token::Comma => "comma",
            Token::Whitespace => "whitespace",
            Token::Text(_) => "text",
        }
    }

    fn text(self) -> Option<&'a str> {
        match self {
            Token::Text(s) => Some(s),
            _ => None,
        }
    }

    fn parsed_text<T: FromStr>(self) -> Option<Result<T, T::Err>> {
        self.text().map(|s| s.trim().parse())
    }

    fn open_paren(self) -> Option<()> {
        matches!(self, Token::OpenParen).then(|| ())
    }

    fn close_paren(self) -> Option<()> {
        matches!(self, Token::CloseParen).then(|| ())
    }
}

struct Lexer<'a> {
    string: &'a str,
    chars: std::iter::Peekable<std::iter::Enumerate<std::str::Chars<'a>>>,
}

impl<'a> Lexer<'a> {
    fn new(string: &'a str) -> Lexer<'a> {
        Lexer {
            string,
            chars: string.chars().enumerate().peekable(),
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let (index, c) = self.chars.next()?;
        Some(match c {
            '(' => Token::OpenParen,
            ')' => Token::CloseParen,
            ',' => Token::Comma,
            c if c.is_whitespace() => {
                while let Some((_, c)) = self.chars.peek() {
                    if !c.is_whitespace() {
                        break;
                    }
                    self.chars.next();
                }
                // In the future, we might want to return the whitespace

                // let end = self
                //     .chars
                //     .peek()
                //     .map_or_else(|| self.string.len(), |(i, _)| *i);
                // Token::Whitespace(&self.string[index..end])

                Token::Whitespace
            }
            _ => {
                while let Some((_, c)) = self.chars.peek() {
                    if c == &'(' || c == &')' || c == &',' {
                        break;
                    }
                    self.chars.next();
                }
                let end = self
                    .chars
                    .peek()
                    .map_or_else(|| self.string.len(), |(i, _)| *i);
                Token::Text(&self.string[index..end])
            }
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventFilterParseError {
    #[error("invalid filter: {0}")]
    InvalidFilter(String),
    #[error("expected token {0:?}, received {1}")]
    ExpectedToken(EventFilterParsable, String),
    #[error("error parsing {0:?}: {1}")]
    ParseError(EventFilterParsable, String),
    #[error("unexpected trailing tokens")]
    TrailingTokens,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EventFilterParsable {
    OpenParen,
    CloseParen,
    CommaOrCloseParen,
    FilterName,
    AgentId,
    EnvId,
    TransactionId,
    CannonId,
    EventKind,
    NodeKey,
    NodeTarget,
}

struct FilterParser<'a> {
    tokens: std::iter::Peekable<Lexer<'a>>,
}

fn expect_token<'a, T>(
    token: Option<Token<'a>>,
    label: EventFilterParsable,
    matcher: impl Fn(Token<'a>) -> Option<T>,
) -> Result<T, EventFilterParseError> {
    use EventFilterParseError::*;
    let token = token.ok_or_else(|| ExpectedToken(label, "EOF".to_string()))?;
    matcher(token).ok_or_else(|| ExpectedToken(label, token.label().to_string()))
}

fn expect_parsed_text<T: FromStr>(
    token: Option<Token>,
    label: EventFilterParsable,
) -> Result<T, EventFilterParseError>
where
    <T as FromStr>::Err: Display,
{
    expect_token(token, label, |token| token.parsed_text::<T>())?
        .map_err(|e| EventFilterParseError::ParseError(label, e.to_string()))
}

fn expect_open_paren(token: Option<Token>) -> Result<(), EventFilterParseError> {
    expect_token(token, EventFilterParsable::OpenParen, |token| {
        token.open_paren()
    })
}

fn expect_close_paren(token: Option<Token>) -> Result<(), EventFilterParseError> {
    expect_token(token, EventFilterParsable::CloseParen, |token| {
        token.close_paren()
    })
}

impl<'a> FilterParser<'a> {
    fn new(str: &'a str) -> Self {
        Self {
            tokens: Lexer::new(str).peekable(),
        }
    }

    fn next(&mut self) -> Option<Token<'a>> {
        self.tokens.next()
    }

    fn expect_parens(
        &mut self,
        filter: impl Fn(&mut Self) -> Result<EventFilter, EventFilterParseError>,
    ) -> Result<EventFilter, EventFilterParseError> {
        self.trim_whitespace();
        expect_open_paren(self.next())?;
        self.trim_whitespace();
        let filter = filter(self)?;
        expect_close_paren(self.next())?;
        Ok(filter)
    }

    fn expect_filter(&mut self) -> Result<EventFilter, EventFilterParseError> {
        self.trim_whitespace();
        use EventFilterParsable as P;
        use EventFilterParseError::*;

        let filter_name = expect_token(self.next(), P::FilterName, |token| token.text())?;

        match filter_name.trim() {
            "unfiltered" => Ok(EventFilter::Unfiltered),
            "any-of" => self.expect_parens(|t| t.expect_filter_vec().map(EventFilter::AnyOf)),
            "all-of" => self.expect_parens(|t| t.expect_filter_vec().map(EventFilter::AllOf)),
            "one-of" => self.expect_parens(|t| t.expect_filter_vec().map(EventFilter::OneOf)),
            "not" => self.expect_parens(|t| Ok(EventFilter::Not(Box::new(t.expect_filter()?)))),
            "agent-is" => self.expect_parens(|t| {
                expect_parsed_text(t.next(), P::AgentId).map(EventFilter::AgentIs)
            }),
            "env-is" => self
                .expect_parens(|t| expect_parsed_text(t.next(), P::EnvId).map(EventFilter::EnvIs)),
            "transaction-is" => self.expect_parens(|t| {
                Ok(EventFilter::TransactionIs(Arc::new(
                    expect_token(t.next(), P::TransactionId, |token| token.text())?.to_string(),
                )))
            }),
            "cannon-is" => self.expect_parens(|t| {
                expect_parsed_text(t.next(), P::CannonId).map(EventFilter::CannonIs)
            }),
            "event-is" => self.expect_parens(|t| {
                expect_parsed_text(t.next(), P::EventKind).map(EventFilter::EventIs)
            }),
            "node-key-is" => self.expect_parens(|t| {
                expect_parsed_text(t.next(), P::NodeKey).map(EventFilter::NodeKeyIs)
            }),
            "node-target-is" => self.expect_parens(|t| {
                expect_parsed_text::<NodeTarget>(t.next(), P::NodeTarget)
                    .map(|t| EventFilter::NodeTargetIs(NodeTargets::One(t)))
            }),

            // Try to parse as an event kind filter as a fallback
            unknown => unknown
                .parse::<EventKindFilter>()
                .map(EventFilter::EventIs)
                .map_err(|_| InvalidFilter(unknown.to_string())),
        }
    }

    fn expect_filter_vec(&mut self) -> Result<Vec<EventFilter>, EventFilterParseError> {
        self.trim_whitespace();
        let mut filters = Vec::new();
        loop {
            match self.tokens.peek() {
                Some(Token::CloseParen) => break,
                Some(_) => {
                    filters.push(self.expect_filter()?);
                    self.trim_whitespace();

                    // Expect either a comma or a close paren
                    match self.tokens.peek() {
                        // This also supports trailing commas
                        Some(Token::Comma) => {
                            self.tokens.next();
                            self.trim_whitespace();
                        }
                        Some(Token::CloseParen) => break,
                        Some(_) => {
                            return Err(EventFilterParseError::ExpectedToken(
                                EventFilterParsable::CommaOrCloseParen,
                                self.tokens.peek().unwrap().label().to_string(),
                            ))
                        }
                        None => {
                            return Err(EventFilterParseError::ExpectedToken(
                                EventFilterParsable::CommaOrCloseParen,
                                "EOF".to_string(),
                            ))
                        }
                    }
                }
                None => {
                    return Err(EventFilterParseError::ExpectedToken(
                        EventFilterParsable::CloseParen,
                        "EOF".to_string(),
                    ))
                }
            }
        }
        Ok(filters)
    }

    /// Remove leading whitespace tokens from the token stream.
    fn trim_whitespace(&mut self) {
        while let Some(Token::Whitespace) = self.tokens.peek() {
            self.tokens.next();
        }
    }

    fn trailing_tokens(&mut self) -> Result<(), EventFilterParseError> {
        self.trim_whitespace();
        if self.tokens.next().is_some() {
            Err(EventFilterParseError::TrailingTokens)
        } else {
            Ok(())
        }
    }
}

impl FromStr for EventFilter {
    type Err = EventFilterParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parser = FilterParser::new(s);
        let filter = parser.expect_filter()?;
        parser.trailing_tokens()?;
        Ok(filter)
    }
}
