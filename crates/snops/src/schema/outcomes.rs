use indexmap::IndexMap;
use promql_parser::{label::Matcher, parser::ast::Expr as PromExpr};
use serde::{de::Visitor, Deserialize};

// TODO: define built-in outcomes here or something

/// A document describing a test's expected outcomes.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub metrics: OutcomeMetrics,
}

pub type OutcomeMetrics = IndexMap<String, OutcomeExpectation>;

/// An outcome expectation; a metric/query, and a way to validate its value
/// after a timeline ends.
#[derive(Deserialize, Debug, Clone)]
pub struct OutcomeExpectation {
    /// The name of the expected outcome, like `network/tps`.
    pub name: String,
    /// A PromQL query that will be used to verify the outcome.
    ///
    /// If unspecified, the metric outcome name used may refer to a built-in
    /// PromQL, which will be used instead.
    pub query: Option<PromQuery>,
    #[serde(flatten)]
    pub validation: OutcomeValidation,
    // TODO: do we want a way to only check certain agents?
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
}

/// A PromQL query.
#[derive(Debug, Clone)]
pub struct PromQuery(pub PromExpr);

impl PromQuery {
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
                promql_parser::parser::parse(v)
                    .map(PromQuery)
                    .map_err(E::custom)
            }
        }

        deserializer.deserialize_str(PromQueryVisitor)
    }
}

pub type OutcomeResults<'a> = Vec<OutcomeResult<'a>>;

pub struct OutcomeResult<'a> {
    pub name: &'a str,
    pub pass: bool, // TODO: need more states than pass/fail?
}
