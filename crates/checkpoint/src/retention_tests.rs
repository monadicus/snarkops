use super::retention::*;
use chrono::{DateTime, TimeDelta};
use std::collections::{BinaryHeap, HashSet};
use std::{num::NonZeroU8, str::FromStr};

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
        if policy.is_ready_with_time(&now, &last_insert) {
            times.insert(now);
            last_insert = now;
            num_added += 1;
        }

        // give a specific interval for "gc" to occur
        if now.signed_duration_since(last_gc) >= gc_interval {
            let rejected = policy.reject_with_time(now, times.iter().collect());
            for time in rejected {
                times.remove(&time);
            }

            last_gc = now;
        }
    }

    println!("FINISH: {:?}", times.iter().collect::<BinaryHeap<_>>());
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
