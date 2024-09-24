use std::{collections::BinaryHeap, fmt::Write, num::NonZeroU8, str::FromStr};

use chrono::{DateTime, TimeDelta, Utc};

/// A comma separated list of retention rules ordered by duration,
/// with the first rule being the shortest
///
/// eg. 4h:1h,1W:U,4W:1D,6M:1W,1Y:1M,U:6M
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub rules: Vec<RetentionRule>,
}

/// An individual rule in a retention policy
/// - 4h:1h - for 4 hours, keep a checkpoint every hour
/// - 1W:U - for 1 week, keep every checkpoint
/// - 4W:1D - for 4 weeks, keep a checkpoint every day
/// - 6M:1W - for 6 months, keep a checkpoint every week
/// - 1Y:1M - for 1 year, keep a checkpoint every month
/// - U:6M - for all time, keep a checkpoint every 6 months
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionRule {
    /// For checkpoints created in this duration...
    pub duration: RetentionSpan,
    /// keep this many
    pub keep: RetentionSpan,
}

impl RetentionPolicy {
    pub fn new(rules: Vec<RetentionRule>) -> Self {
        Self { rules }
    }

    /// Returns true if the policy is ready to be applied based on a given time
    pub fn is_ready_with_time(&self, new_time: &DateTime<Utc>, last_time: &DateTime<Utc>) -> bool {
        // if there are no rules, the policy does not apply
        let Some(rule) = self.rules.first() else {
            return false;
        };

        // if the first rule is unlimited, the policy is always ready
        let Some(keep) = rule.keep.as_delta() else {
            return true;
        };

        // amount of time since the last checkpoint
        let delta = new_time.signed_duration_since(last_time);

        // if the time since the last checkpoint is greater than the minimum delta
        // the policy indicates a new checkpoint should be created
        delta >= keep
    }

    /// Receives a list of checkpoint times, and returns a list of times to
    /// reject
    pub fn reject(&self, times: Vec<&DateTime<Utc>>) -> Vec<DateTime<Utc>> {
        self.reject_with_time(Utc::now(), times)
    }
    /// Receives a list of checkpoint times, and returns a list of times to
    /// reject given a time
    pub fn reject_with_time(
        &self,
        now: DateTime<Utc>,
        times: Vec<&DateTime<Utc>>,
    ) -> Vec<DateTime<Utc>> {
        // if the policy is empty, we should technically reject ALL checkpoints but
        // for safety we will not reject any
        if self.rules.is_empty() || times.is_empty() {
            return Vec::new();
        }

        let mut rejected = Vec::new();

        // ALGORITHM
        // 1. walk backwards through rules and times
        // 2. keep track of the last kept time for each rule
        // 3. if the last kept time is outside the duration of the current rule reject
        //    the last kept time and promote the next time to the last kept time
        // 4. if the current rule does not encompass both times move to the next rule
        // 5. if difference between last kept time and current time is smaller than the
        //    keep time, reject it
        // 6. if the time was not rejected, it becomes the new last kept time

        // TODO: this is bugged atm - it's keeping the last checkpoint

        // step 1 - walk backwards through rules and times
        let mut rules = self.rules.iter().rev().peekable();

        let mut times = times
            .into_iter()
            .collect::<BinaryHeap<_>>()
            .into_sorted_vec()
            .into_iter()
            .rev()
            .peekable();

        // step 2 - keep track of the last kept time
        let mut last_kept = times.next().unwrap(); // is_empty checked at the beginning of the fn
        let mut curr_rule = rules.next().unwrap(); // is_empty checked at the beginning of the fn

        'outer: while let Some(time) = times.peek().cloned() {
            let delta = now.signed_duration_since(time);
            let last_delta = now.signed_duration_since(last_kept);

            // step 3 - if the last time is outside the duration of the current rule, reject
            // it
            match curr_rule.duration.as_delta() {
                Some(duration) if last_delta > duration => {
                    /* println!(
                        "STEP 3 {curr_rule}: {last_kept} is older than ({}) > {}",
                        last_delta.num_seconds() / 60,
                        duration.num_seconds() / 60
                    ); */
                    rejected.push(*last_kept);
                    // promote the next time to the last kept time
                    last_kept = time;
                    times.next();
                    continue;
                }
                _ => {}
            }

            // check if we should move to the next rule
            while let Some(RetentionRule { duration, .. }) = rules.peek() {
                // if both rules have the same duration, continue to the next rule
                // this is another case of configuration mishaps
                //
                // additionallly, if the second to last rule is unlimited, continue to the next
                // rule you should not be writing policies with multiple
                // unlimited rules
                if &curr_rule.duration == duration || duration == &RetentionSpan::Unlimited {
                    curr_rule = rules.next().unwrap();
                    continue;
                }

                if let Some(next_duration) = duration.as_delta() {
                    // step 4 - if the current rule does not encompass both times, move to the next
                    // rule

                    // continue because both times are within the current rule
                    if delta >= next_duration && last_delta >= next_duration {
                        break;
                    }

                    // update the last step time if the current time is within the next duration
                    if delta < next_duration {
                        last_kept = time;
                        times.next();
                    }

                    curr_rule = rules.next().unwrap();
                    continue 'outer;
                }
            }

            // keep the current time if the current rule is unlimited
            let Some(keep) = curr_rule.keep.as_delta() else {
                last_kept = time;
                times.next();
                continue;
            };

            // step 5 - if the difference between the last kept time and the
            // current time is smaller than the keep time, reject it
            if last_kept.signed_duration_since(time) < keep {
                /*  println!(
                    "STEP 5 {curr_rule}: {last_kept} - {time} ({}) < {}",
                    last_kept.signed_duration_since(time).num_seconds() / 60,
                    keep.num_seconds() / 60
                ); */
                rejected.push(*time);
                times.next();
                continue;
            }

            // step 6 - if the time was not rejected, it becomes the new last kept time
            /* println!(
                "OK {curr_rule}: {last_kept} - {time} ({}) >= {}",
                last_kept.signed_duration_since(time).num_seconds() / 60,
                keep.num_seconds() / 60
            ); */
            last_kept = time;
            times.next();
        }

        rejected
    }
}

impl Default for RetentionPolicy {
    /// The default policy is intended to align with the test cases provided by
    /// Aleo.
    fn default() -> Self {
        Self {
            rules: [
                "4h:1h", // for 4 hours, keep a checkpoint every hour
                "1D:8h", // for 1 day, keep a checkpoint every 8 hours
                "1W:1D", // for 1 week, keep a checkpoint every day
                "4W:1W", // for 4 weeks, keep a checkpoint every week
                "4M:1M", // for 4 months, keep a checkpoint every month
                "U:1Y",  // for all time, keep a checkpoint every year
            ]
            .into_iter()
            .map(RetentionRule::from_str)
            .collect::<Result<_, _>>()
            .unwrap(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionSpan {
    /// U
    Unlimited,
    /// 1m
    Minute(NonZeroU8),
    /// 1h
    Hour(NonZeroU8),
    /// 1D
    Day(NonZeroU8),
    /// 1W
    Week(NonZeroU8),
    /// 1M
    Month(NonZeroU8),
    /// 1Y
    Year(NonZeroU8),
}

impl RetentionSpan {
    pub fn as_delta(&self) -> Option<TimeDelta> {
        match self {
            RetentionSpan::Unlimited => None,
            RetentionSpan::Minute(value) => TimeDelta::try_minutes(value.get() as i64),
            RetentionSpan::Hour(value) => TimeDelta::try_hours(value.get() as i64),
            RetentionSpan::Day(value) => TimeDelta::try_days(value.get() as i64),
            RetentionSpan::Week(value) => TimeDelta::try_weeks(value.get() as i64),
            RetentionSpan::Month(value) => TimeDelta::try_days(value.get() as i64 * 30),
            RetentionSpan::Year(value) => TimeDelta::try_days(value.get() as i64 * 365),
        }
    }

    // get the timestamp for the start of the retention span
    pub fn as_timestamp(&self) -> Option<i64> {
        Utc::now().timestamp().checked_sub(match self {
            RetentionSpan::Unlimited => return None,
            RetentionSpan::Minute(value) => value.get() as i64 * 60,
            RetentionSpan::Hour(value) => value.get() as i64 * 3600,
            RetentionSpan::Day(value) => value.get() as i64 * 3600 * 24,
            RetentionSpan::Week(value) => value.get() as i64 * 3600 * 24 * 7,
            RetentionSpan::Month(value) => value.get() as i64 * 3600 * 24 * 30,
            RetentionSpan::Year(value) => value.get() as i64 * 3600 * 24 * 365,
        })
    }
}

impl FromStr for RetentionPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rules = s
            .split(',')
            .enumerate()
            .filter(|(_, s)| !s.is_empty())
            .map(|(i, rule)| {
                rule.parse()
                    .map_err(|e| format!("parse error in rule {} ({rule}): {e}", i + 1))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RetentionPolicy::new(rules))
    }
}

impl std::fmt::Display for RetentionPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, rule) in self.rules.iter().enumerate() {
            if i > 0 {
                f.write_char(',')?;
            }
            rule.fmt(f)?;
        }
        Ok(())
    }
}

impl FromStr for RetentionRule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (duration, keep) = s.split_at(s.find(':').ok_or("missing ':'".to_owned())?);
        Ok(RetentionRule {
            duration: duration.parse().map_err(|e| format!("duration: {e}"))?,
            keep: keep[1..].parse().map_err(|e| format!("keep: {e}"))?,
        })
    }
}

impl std::fmt::Display for RetentionRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.duration, self.keep)
    }
}

impl FromStr for RetentionSpan {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let unit = s.chars().last().ok_or("missing unit")?;
        if unit == 'U' {
            if s.len() != 1 {
                return Err("invalid value for unlimited".to_owned());
            }
            return Ok(RetentionSpan::Unlimited);
        }
        let value = s[..s.len() - 1]
            .parse()
            .map_err(|e| format!("invalid value '{}': {e}", &s[..s.len() - 1]))?;

        match unit {
            'm' => Ok(RetentionSpan::Minute(value)),
            'h' => Ok(RetentionSpan::Hour(value)),
            'D' => Ok(RetentionSpan::Day(value)),
            'W' => Ok(RetentionSpan::Week(value)),
            'M' => Ok(RetentionSpan::Month(value)),
            'Y' => Ok(RetentionSpan::Year(value)),
            _ => Err("invalid unit".to_owned()),
        }
    }
}

impl std::fmt::Display for RetentionSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RetentionSpan::Unlimited => write!(f, "U"),
            RetentionSpan::Minute(value) => write!(f, "{}m", value),
            RetentionSpan::Hour(value) => write!(f, "{}h", value),
            RetentionSpan::Day(value) => write!(f, "{}D", value),
            RetentionSpan::Week(value) => write!(f, "{}W", value),
            RetentionSpan::Month(value) => write!(f, "{}M", value),
            RetentionSpan::Year(value) => write!(f, "{}Y", value),
        }
    }
}

#[cfg(feature = "serde")]
macro_rules! impl_serde {
    ($($ty:ty),*) => {
        $(
            impl serde::Serialize for $ty {
                fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                    self.to_string().serialize(serializer)
                }
            }

            impl<'de> serde::Deserialize<'de> for $ty {
                fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                    String::deserialize(deserializer)?
                        .parse()
                        .map_err(serde::de::Error::custom)
                }
            }
        )*
    };
}

#[cfg(feature = "serde")]
impl_serde!(RetentionSpan, RetentionRule);

#[cfg(feature = "serde")]
impl serde::Serialize for RetentionPolicy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for RetentionPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let string = String::deserialize(deserializer)?;
        if string == "default" {
            return Ok(RetentionPolicy::default());
        }
        string.parse().map_err(serde::de::Error::custom)
    }
}
