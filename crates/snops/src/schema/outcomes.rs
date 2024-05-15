use std::{
    collections::HashMap,
    fmt::Display,
    io::{Read, Write},
};

use lazy_static::lazy_static;
use promql_parser::{label::Matcher, parser::ast::Expr as PromExpr};
use serde::{de::Visitor, Deserialize, Serialize};
use snops_common::{
    format::{DataFormat, DataReadError, DataWriteError},
    state::MetricId,
};

use super::error::SchemaError;

/// A document associating a metric name with a PromQL query that can be used
/// later in the same environment.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub metrics: MetricQueries,
}

pub type MetricQueries = HashMap<MetricId, PromQuery>;

/// An outcome expectation; a metric/query, and a way to validate its value
/// after a timeline ends.
#[derive(Deserialize, Debug, Clone)]
pub struct OutcomeExpectation {
    /// A PromQL query that will be used to verify the outcome.
    ///
    /// If unspecified, the metric outcome name used may refer to a built-in
    /// PromQL or a query defined in an `outcomes` document, if one exists.
    pub query: Option<PromQuery>,
    #[serde(flatten)]
    pub validation: OutcomeValidation,
}

/// An outcome validation method.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum OutcomeValidation {
    /// The outcome value must be within a particular range.
    Range {
        /// The minimum value that the outcome value can be and pass.
        min: Option<f64>,
        /// The maximum value that the outcome value can be and pass.
        max: Option<f64>,
    },

    /// The outcome value must be equal (or roughly equal) to a particular
    /// value.
    Eq {
        /// A value that the outcome value must be and pass.
        ///
        /// Use `epsilon` to control a maximum delta between this value and
        /// allowed values, so that the allowed range becomes `(eq -
        /// epsilon) <= outcome <= (eq + epsilon)`.
        eq: f64,
        /// See `eq`.
        epsilon: Option<f64>,
    },
}

impl OutcomeValidation {
    /// Validate a number given outcome validation constraints.
    pub fn validate(&self, value: f64) -> bool {
        match self {
            Self::Range { min, max } => {
                if matches!(min, Some(min) if value.lt(min)) {
                    return false;
                }
                if matches!(max, Some(max) if value.gt(max)) {
                    return false;
                }
                true
            }

            Self::Eq { eq, epsilon } => {
                let epsilon = epsilon.unwrap_or(f64::EPSILON);
                ((eq - epsilon)..=(eq + epsilon)).contains(&value)
            }
        }
    }

    pub fn show_validation(&self, value: f64) -> (bool, String) {
        let success = self.validate(value);
        (
            success,
            if success {
                format!("✅ pass, {value} is {self}")
            } else {
                format!("⚠️ expected {value} to be {self}")
            },
        )
    }
}

impl Display for OutcomeValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use OutcomeValidation::*;
        match self {
            Range { min, max } => match (min, max) {
                (Some(min), Some(max)) => write!(f, "between {min} and {max}"),
                (Some(min), None) => write!(f, "at least {min}"),
                (None, Some(max)) => write!(f, "at most {max}"),
                (None, None) => write!(f, "anything"),
            },
            Eq { eq, epsilon } => match epsilon {
                Some(epsilon) => write!(f, "equal to {eq} ± {epsilon}"),
                None => write!(f, "equal to {eq}"),
            },
        }
    }
}

/// A PromQL query.
#[derive(Debug, Clone)]
pub struct PromQuery(PromExpr);

impl PromQuery {
    /// Parse a PromQL query into a `PromQuery`.
    pub fn new(query: &str) -> Result<Self, SchemaError> {
        promql_parser::parser::parse(query)
            .map(Self)
            .map_err(SchemaError::QueryParse)
    }

    pub fn builtin(name: &str) -> Option<&'static Self> {
        macro_rules! builtins {
            { $($name:literal : $query:literal),+ , } => {
                lazy_static! {
                    static ref QUERY_BUILTINS: HashMap<&'static str, PromQuery> = [
                        $(($name, PromQuery::new($query).unwrap())),+
                    ]
                    .into_iter()
                    .collect();
                }
            }
        }

        builtins! {
            "network/tps": "avg(rate(snarkos_blocks_transactions_total[10s]))", // TODO: time
        }

        QUERY_BUILTINS.get(name)
    }

    /// Fetch the inner PromQL expression from this query.
    pub fn into_inner(self) -> PromExpr {
        self.0
    }

    /// Inject environment label matchers into the query.
    pub fn add_matchers(&mut self, matchers: &[Matcher]) {
        Self::inject_matchers(&mut self.0, matchers);
    }

    fn inject_matchers(expr: &mut PromExpr, matchers: &[Matcher]) {
        macro_rules! inject {
            ($into:expr) => {
                Self::inject_matchers(&mut $into, matchers)
            };
            ($into:expr, $($into2:expr),+) => {
                {
                    inject!($into);
                    inject!($($into2),+);
                }
            };
        }

        // TODO: we may only want to inject matchers on metrics that look like
        // `snarkos_XXXX`
        match expr {
            PromExpr::Aggregate(expr) => inject!(expr.expr),
            PromExpr::Unary(expr) => inject!(expr.expr),
            PromExpr::Binary(expr) => inject!(expr.lhs, expr.rhs),
            PromExpr::Paren(expr) => inject!(expr.expr),
            PromExpr::Subquery(expr) => inject!(expr.expr),
            PromExpr::NumberLiteral(_) => (),
            PromExpr::StringLiteral(_) => (),
            PromExpr::VectorSelector(selector) => {
                selector.matchers.matchers.extend(matchers.iter().cloned());
            }
            PromExpr::MatrixSelector(selector) => selector
                .vs
                .matchers
                .matchers
                .extend(matchers.iter().cloned()),
            PromExpr::Call(call) => {
                call.args
                    .args
                    .iter_mut()
                    .for_each(|arg| Self::inject_matchers(arg, matchers));
            }
            PromExpr::Extension(_) => (),
        }
    }
}

impl<'de> Deserialize<'de> for PromQuery {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PromQueryVisitor;

        impl<'de> Visitor<'de> for PromQueryVisitor {
            type Value = PromQuery;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a PromQL query")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                PromQuery::new(v).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(PromQueryVisitor)
    }
}

impl Serialize for PromQuery {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        format!("{}", self.0).serialize(serializer)
    }
}

impl DataFormat for PromQuery {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        format!("{}", self.0).write_data(writer)
    }

    fn read_data<R: Read>(reader: &mut R, _: &Self::Header) -> Result<Self, DataReadError> {
        let buf = String::read_data(reader, &())?;
        PromQuery::new(&buf).map_err(DataReadError::custom)
    }
}
