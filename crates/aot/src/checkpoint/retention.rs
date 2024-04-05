use std::{fmt::Display, num::NonZeroU8, str::FromStr};

use chrono::{DateTime, TimeDelta, Utc};
use snarkvm::console::program::Itertools;

#[derive(Debug, Clone, PartialEq, Eq)]
/// A comma separated list of retention rules ordered by duration,
/// with the first rule being the shortest
///
/// eg. 4h:1h,1W:U,4W:1D,6M:1W,1Y:1M,U:6M
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

    /// Returns true if the policy is ready to be applied based on the current time
    pub fn is_ready(&self, last_time: &DateTime<Utc>) -> bool {
        self.is_ready_with_time(Utc::now(), last_time)
    }

    /// Returns true if the policy is ready to be applied based on a given time
    pub fn is_ready_with_time(&self, now: DateTime<Utc>, last_time: &DateTime<Utc>) -> bool {
        // if there are no rules, the policy does not apply
        let Some(rule) = self.rules.first() else {
            return false;
        };

        // if the first rule is unlimited, the policy is always ready
        let Some(keep) = rule.keep.as_duration() else {
            return true;
        };

        // amount of time since the last checkpoint
        let delta = now.signed_duration_since(last_time);

        // if the time since the last checkpoint is greater than the minimum delta
        // the policy indicates a new checkpoint should be created
        delta >= keep
    }

    /// Receives a list of checkpoint times, and returns a list of times to reject
    pub fn reject(&self, times: Vec<DateTime<Utc>>) -> Vec<DateTime<Utc>> {
        self.reject_with_time(Utc::now(), times)
    }
    /// Receives a list of checkpoint times, and returns a list of times to reject given a time
    pub fn reject_with_time(
        &self,
        now: DateTime<Utc>,
        times: Vec<DateTime<Utc>>,
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
        // 3. if the last kept time is outside the duration of the current rule
        //    reject the last kept time and promote the next time to the last kept time
        // 4. if the current rule does not encompass both times
        //    move to the next rule
        // 5. if difference between last kept time and current time is
        //    smaller than the keep time, reject it
        // 6. if the time was not rejected, it becomes the new last kept time

        // step 1 - walk backwards through rules and times
        let mut rules = self.rules.iter().rev().peekable();
        let mut times = times.into_iter().sorted().peekable();

        // println!("\n[debug] times: {:?}", times.clone().collect_vec());

        // step 2 - keep track of the last kept time
        let mut last_kept = times.next().unwrap();
        let mut curr_rule = rules.next().unwrap();

        'outer: while let Some(time) = times.peek().cloned() {
            // println!("VISIT {time}");
            let delta = now.signed_duration_since(time);
            let last_delta = now.signed_duration_since(last_kept);

            // step 3 - if the last time is outside the duration of the current rule, reject it
            match curr_rule.duration.as_duration() {
                Some(duration) if last_delta > duration => {
                    /* println!(
                        "STEP 3 {curr_rule}: {last_kept} is older than ({}) > {}",
                        last_delta.num_seconds() / 3600,
                        duration.num_seconds() / 3600
                    ); */
                    rejected.push(last_kept);
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
                // additionallly, if the second to last rule is unlimited, continue to the next rule
                // you should not be writing policies with multiple unlimited rules
                if &curr_rule.duration == duration || duration == &RetentionSpan::Unlimited {
                    curr_rule = rules.next().unwrap();
                    continue;
                }

                if let Some(next_duration) = duration.as_duration() {
                    // step 4 - if the current rule does not encompass both times, move to the next rule

                    // continue because both times are within the current rule
                    if delta >= next_duration && last_delta >= next_duration {
                        break;
                    }

                    // update the last step time if the current time is within the next duration
                    if delta < next_duration {
                        // println!("OK {curr_rule}: {last_kept}");
                        last_kept = time;
                        times.next();
                    }

                    curr_rule = rules.next().unwrap();
                    continue 'outer;
                }
            }

            // keep the current time if the current rule is unlimited
            let Some(keep) = curr_rule.keep.as_duration() else {
                // println!("{curr_rule}: keep is unlimited, keeping {time}");
                last_kept = time;
                times.next();
                continue;
            };

            // step 5 - if the difference between the last kept time and the
            // current time is smaller than the keep time, reject it
            if time.signed_duration_since(last_kept) < keep {
                /* println!(
                    "STEP 5 {curr_rule}: {last_kept} - {time} ({}) < {}",
                    time.signed_duration_since(last_kept).num_seconds() / 3600,
                    keep.num_seconds() / 3600
                ); */
                rejected.push(time);
                times.next();
                continue;
            }

            // step 6 - if the time was not rejected, it becomes the new last kept time
            /*  println!(
                "OK {curr_rule}: {last_kept} - {time} ({}) >= {}",
                (time - last_kept).num_seconds() / 3600,
                keep.num_seconds() / 3600
            ); */
            last_kept = time;
            times.next();
        }

        rejected
    }
}

impl Default for RetentionPolicy {
    /// The default policy is intended to align with the test cases provided by Aleo.
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

impl Display for RetentionPolicy {
    fn fmt(&self, f: &mut snarkvm::prelude::Formatter<'_>) -> snarkvm::prelude::fmt::Result {
        write!(
            f,
            "{}",
            self.rules
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
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

impl Display for RetentionRule {
    fn fmt(&self, f: &mut snarkvm::prelude::Formatter<'_>) -> snarkvm::prelude::fmt::Result {
        write!(f, "{}:{}", self.duration, self.keep)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionSpan {
    /// U
    Unlimited,
    /// 1H
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
    pub fn as_duration(&self) -> Option<TimeDelta> {
        match self {
            RetentionSpan::Unlimited => None,
            RetentionSpan::Hour(value) => TimeDelta::try_hours(value.get() as i64),
            RetentionSpan::Day(value) => TimeDelta::try_days(value.get() as i64),
            RetentionSpan::Week(value) => TimeDelta::try_weeks(value.get() as i64),
            RetentionSpan::Month(value) => TimeDelta::try_days(value.get() as i64 * 30),
            RetentionSpan::Year(value) => TimeDelta::try_days(value.get() as i64 * 365),
        }
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
            'h' => Ok(RetentionSpan::Hour(value)),
            'D' => Ok(RetentionSpan::Day(value)),
            'W' => Ok(RetentionSpan::Week(value)),
            'M' => Ok(RetentionSpan::Month(value)),
            'Y' => Ok(RetentionSpan::Year(value)),
            _ => Err("invalid unit".to_owned()),
        }
    }
}

impl Display for RetentionSpan {
    fn fmt(&self, f: &mut snarkvm::prelude::Formatter<'_>) -> snarkvm::prelude::fmt::Result {
        match self {
            RetentionSpan::Unlimited => write!(f, "U"),
            RetentionSpan::Hour(value) => write!(f, "{}h", value),
            RetentionSpan::Day(value) => write!(f, "{}D", value),
            RetentionSpan::Week(value) => write!(f, "{}W", value),
            RetentionSpan::Month(value) => write!(f, "{}M", value),
            RetentionSpan::Year(value) => write!(f, "{}Y", value),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;

    macro_rules! day {
        ($e:expr) => {
            TimeDelta::try_days($e).unwrap()
        };
    }
    macro_rules! hr {
        ($e:expr) => {
            TimeDelta::try_hours($e).unwrap()
        };
    }
    macro_rules! min {
        ($e:expr) => {
            TimeDelta::try_minutes($e).unwrap()
        };
    }

    /// Walk through a given policy and return the number of kept checkpoints
    fn walk_policy(
        policy: &RetentionPolicy,
        duration: TimeDelta,
        add_interval: TimeDelta,
        gc_interval: TimeDelta,
    ) -> (usize, usize) {
        let mut times = HashSet::new();
        let mut now = DateTime::UNIX_EPOCH;
        let mut last_gc = now;
        let mut last_insert = now;

        let mut num_added = 0;

        while now.signed_duration_since(DateTime::UNIX_EPOCH) < duration {
            now += add_interval;

            // if the policy indicates it's okay to create a new checkpoint, add it
            if policy.is_ready_with_time(now, &last_insert) {
                // println!("adding checkpoint {now}");
                times.insert(now);
                last_insert = now;
                num_added += 1;
            }

            // give a specific interval for "gc" to occur
            if now.signed_duration_since(last_gc) >= gc_interval {
                let rejected = policy.reject_with_time(now, times.clone().into_iter().collect());
                for time in rejected {
                    // println!("removing checkpoint {time}");
                    times.remove(&time);
                }

                last_gc = now;
            }
        }

        println!(
            "FINISH: {:?}",
            times.iter().sorted().rev().collect::<Vec<_>>()
        );
        (num_added, times.len())
    }

    #[test]
    #[rustfmt::skip]
    fn parse_span() {
        assert_eq!(RetentionSpan::from_str("U").unwrap(), RetentionSpan::Unlimited);
        assert_eq!(RetentionSpan::from_str("1h").unwrap(), RetentionSpan::Hour(NonZeroU8::new(1).unwrap()));
        assert_eq!(RetentionSpan::from_str("1D").unwrap(), RetentionSpan::Day(NonZeroU8::new(1).unwrap()));
        assert_eq!(RetentionSpan::from_str("1W").unwrap(), RetentionSpan::Week(NonZeroU8::new(1).unwrap()));
        assert_eq!(RetentionSpan::from_str("1M").unwrap(), RetentionSpan::Month(NonZeroU8::new(1).unwrap()));
        assert_eq!(RetentionSpan::from_str("1Y").unwrap(), RetentionSpan::Year(NonZeroU8::new(1).unwrap()));
    }

    #[test]
    fn parse_rule() {
        assert_eq!(
            RetentionRule::from_str("4h:1h"),
            Ok(RetentionRule {
                duration: RetentionSpan::Hour(NonZeroU8::new(4).unwrap()),
                keep: RetentionSpan::Hour(NonZeroU8::new(1).unwrap())
            })
        );
    }

    macro_rules! policy_test {
        ($name:ident, $policy:expr, $duration:expr, $add_interval:expr, $gc_interval:expr, + $added:expr, = $kept:expr) => {
            #[test]
            fn $name() {
                let (num_added, num_kept) = walk_policy(
                    &RetentionPolicy::from_str($policy).unwrap(),
                    $duration,
                    $add_interval,
                    $gc_interval,
                );
                assert_eq!(
                    (num_added, num_kept),
                    ($added, $kept),
                    "added (found {num_added}, expected {}), kept (found {num_kept}, expected {})",
                    $added,
                    $kept
                );
            }
        };
    }

    // format: policy, test_duration, add_interval, gc_interval, # added, # rejected, # kept

    // these look like they should have 4 entries, but there are actually 4 hours between
    // all of the tests...
    // 24, 23, 22, 21, 20
    policy_test!(one_day_4h1h, "4h:1h", day!(1), min!(1), hr!(1), + 24, = 5);
    // 24, 22, 20
    policy_test!(one_day_4h2h, "4h:2h", day!(1), min!(1), hr!(1), + 12, = 3);
    policy_test!(one_day_u2h, "U:2h", day!(1), min!(1), hr!(1), + 12, = 12);

    // the same tests as above, but the garbage collection is delayed, which results
    // in a slightly different compaction
    policy_test!(one_day_4h1h_delay, "4h:1h", day!(1), min!(1), day!(1), + 24, = 5);
    policy_test!(one_day_4h2h_delay, "4h:2h", day!(1), min!(1), day!(1), + 12, = 3);
    policy_test!(one_day_u2h_delay, "U:2h", day!(1), min!(1), day!(1), + 12, = 12);

    // keep 4 hourly checkpoints, every 4 hours for the last 8 hours (), 4 + 1 total
    // 24, 23, 22, 21, 17
    policy_test!(one_day_4h1h_8h4h, "4h:1h,8h:4h", day!(1), min!(1), hr!(1), + 24, = 5);
    // 24, 23, 22, 21, 20, 16
    policy_test!(one_day_4h1h_8h4h_delay, "4h:1h,8h:4h", day!(1), min!(1), day!(1), + 24, = 6);

    // 08T00, 07T23, 07T22, 07T21, 07T18, 07T14, 07T02, 06T14, 06T02
    policy_test!(one_day_spaced, "4h:1h,8h:4h,2D:12h", day!(7), hr!(1), hr!(1), + (24 * 7), = 9);
    // 08T00, 07T23, 07T22, 07T21, 07T17, 07T13, 07T01, 06T13, 06T01
    policy_test!(one_day_spaced_delay, "4h:1h,8h:4h,2D:12h", day!(7), hr!(1), day!(1), + (24 * 7), = 9);
}
