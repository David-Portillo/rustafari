//! A cron schedule builder: five fields in, the next runs out.
//!
//! Standard five-field cron — minute, hour, day of month, month, day of week —
//! with the usual syntax in each: `*`, `5`, `1-5`, `*/15`, `1-5/2`, `mon`,
//! `jan`, and comma-separated lists of any of those.
//!
//! Everything is UTC. That is a deliberate limitation, not an oversight: it
//! removes time zones and daylight saving from the calculation entirely, which
//! is where schedule previews usually go wrong, and it costs no dependency —
//! the civil-calendar arithmetic below is exact and self-contained.
//!
//! The day rule is Vixie cron's, and it surprises people: when **both** day of
//! month and day of week are restricted, a day matches if **either** does. With
//! only one restricted, that one must match. `0 0 1 * MON` therefore fires on
//! the first of the month *and* every Monday, not on Mondays that fall on the
//! first.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::spec::*;

pub struct Cron;

const OPTIONS: &[OptionSpec] = &[
    OptionSpec::Text {
        id: "minute",
        label: "Minute",
        placeholder: "*",
        default: "0",
    },
    OptionSpec::Text {
        id: "hour",
        label: "Hour",
        placeholder: "*",
        default: "9",
    },
    OptionSpec::Text {
        id: "dom",
        label: "Day",
        placeholder: "*",
        default: "*",
    },
    OptionSpec::Text {
        id: "month",
        label: "Month",
        placeholder: "*",
        default: "*",
    },
    OptionSpec::Text {
        id: "dow",
        label: "Weekday",
        placeholder: "*",
        default: "MON-FRI",
    },
    OptionSpec::Number {
        id: "runs",
        label: "Runs to preview",
        min: 1,
        max: 50,
        default: 5,
    },
];

impl Tool for Cron {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "cron",
            name: "Cron Builder",
            category: Category::Generators,
            description: "Build a cron schedule field by field and see when it will actually run.",
            keywords: &["cron", "crontab", "schedule", "job", "timer", "next run"],
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::None
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn produces(&self, _opts: &Options) -> Format {
        Format::Plain
    }

    fn run(&self, _input: Input<'_>, opts: &Options) -> ToolResult {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        report(opts, now)
    }
}

/// Split out from `run` so tests can pin "now" and assert exact dates.
fn report(opts: &Options, now: i64) -> ToolResult {
    let fields = [
        opts.text("minute").trim(),
        opts.text("hour").trim(),
        opts.text("dom").trim(),
        opts.text("month").trim(),
        opts.text("dow").trim(),
    ];
    // An empty box means "no constraint" rather than an error, so the tool is
    // usable while you are still typing.
    let fields = fields.map(|f| if f.is_empty() { "*" } else { f });
    let schedule = Schedule::parse(&fields)?;

    let mut out = String::new();
    let _ = writeln!(out, "{}\n", fields.join(" "));
    let _ = writeln!(out, "{}\n", schedule.describe());

    let count = opts.number("runs").clamp(1, 50) as usize;
    let runs = schedule.next_runs(now, count);

    if runs.is_empty() {
        out.push_str("NEXT RUNS (UTC)\n  Never — no date in the next four years matches.");
        return Ok(out);
    }

    let _ = writeln!(out, "NEXT {} RUNS (UTC)", runs.len());
    for stamp in runs {
        let (y, m, d) = civil_from_days(stamp.div_euclid(86_400));
        let seconds = stamp.rem_euclid(86_400);
        let _ = writeln!(
            out,
            "  {}  {y:04}-{m:02}-{d:02}  {:02}:{:02}",
            WEEKDAYS[weekday(stamp.div_euclid(86_400)) as usize],
            seconds / 3600,
            (seconds % 3600) / 60,
        );
    }
    Ok(out)
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTH_NAMES: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const DAY_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// Which values each field allows, plus whether the field was left open, which
/// the day rule needs to know.
struct Schedule {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    doms: Vec<u32>,
    months: Vec<u32>,
    dows: Vec<u32>,
    dom_restricted: bool,
    dow_restricted: bool,
}

impl Schedule {
    fn parse(fields: &[&str; 5]) -> Result<Self, ToolError> {
        Ok(Schedule {
            minutes: parse_field(fields[0], 0, 59, &[], "Minute")?,
            hours: parse_field(fields[1], 0, 23, &[], "Hour")?,
            doms: parse_field(fields[2], 1, 31, &[], "Day")?,
            months: parse_field(fields[3], 1, 12, &MONTH_NAMES, "Month")?,
            dows: parse_field(fields[4], 0, 6, &DAY_NAMES, "Weekday")?,
            dom_restricted: fields[2] != "*",
            dow_restricted: fields[4] != "*",
        })
    }

    /// Vixie cron's day rule — see the module docs.
    fn day_matches(&self, days: i64) -> bool {
        let (_, month, day) = civil_from_days(days);
        if !self.months.contains(&month) {
            return false;
        }
        let by_dom = self.doms.contains(&day);
        let by_dow = self.dows.contains(&weekday(days));
        match (self.dom_restricted, self.dow_restricted) {
            (true, true) => by_dom || by_dow,
            (true, false) => by_dom,
            (false, true) => by_dow,
            (false, false) => true,
        }
    }

    /// Timestamps of the next `count` runs strictly after `after`.
    ///
    /// Walks whole days and only descends into a day that matches, so an
    /// expression that fires once a year costs a few hundred cheap checks
    /// rather than half a million minute-by-minute ones.
    fn next_runs(&self, after: i64, count: usize) -> Vec<i64> {
        let mut found = Vec::with_capacity(count);
        // A cron expression that matches nothing (30 February) must terminate.
        // Four years clears any leap-year cycle.
        const HORIZON_DAYS: i64 = 366 * 4;

        let start = after + 60 - after.rem_euclid(60); // next whole minute
        let start_day = start.div_euclid(86_400);

        for day in start_day..start_day + HORIZON_DAYS {
            if !self.day_matches(day) {
                continue;
            }
            let midnight = day * 86_400;
            for &hour in &self.hours {
                for &minute in &self.minutes {
                    let stamp = midnight + i64::from(hour) * 3600 + i64::from(minute) * 60;
                    if stamp < start {
                        continue;
                    }
                    found.push(stamp);
                    if found.len() == count {
                        return found;
                    }
                }
            }
        }
        found
    }

    /// Plain English, good enough to catch a mistake before it ships.
    fn describe(&self) -> String {
        let time = match (self.minutes.len(), self.hours.len()) {
            (1, 1) => format!("At {:02}:{:02}", self.hours[0], self.minutes[0]),
            (_, 1) => format!(
                "At {} past {:02}:00",
                list(
                    &self
                        .minutes
                        .iter()
                        .map(|m| format!("{m:02}"))
                        .collect::<Vec<_>>()
                ),
                self.hours[0]
            ),
            (1, _) if self.minutes[0] == 0 => format!(
                "On the hour, at {}",
                list(
                    &self
                        .hours
                        .iter()
                        .map(|h| format!("{h:02}:00"))
                        .collect::<Vec<_>>()
                )
            ),
            (1, _) => format!(
                "At {} minutes past {}",
                self.minutes[0],
                list(
                    &self
                        .hours
                        .iter()
                        .map(|h| format!("{h:02}:00"))
                        .collect::<Vec<_>>()
                )
            ),
            (60, 24) => "Every minute".to_owned(),
            (m, 24) => format!("{m} times an hour, every hour"),
            (m, h) => format!("{m} times an hour, during {h} hours of the day"),
        };

        let mut parts = vec![time];

        if self.dow_restricted {
            parts.push(match self.dows.len() {
                7 => "every day".to_owned(),
                _ => format!("on {}", list(&ranges(&self.dows, full_day))),
            });
        }
        if self.dom_restricted {
            parts.push(format!(
                "on the {} of the month",
                list(&self.doms.iter().map(|d| ordinal(*d)).collect::<Vec<_>>())
            ));
        }
        if self.months.len() != 12 {
            parts.push(format!(
                "in {}",
                list(&ranges(&self.months, |m| MONTHS[(m - 1) as usize].to_owned()))
            ));
        }

        let mut text = parts.join(", ");
        if self.dom_restricted && self.dow_restricted {
            text.push_str(" — either condition is enough, which is how cron works");
        }
        text.push('.');
        text
    }
}

/// Collapses contiguous values into "A through B", so `MON-FRI` describes
/// itself as a range rather than as a five-item list. Runs of two stay
/// separate, because "Monday through Tuesday" is worse than "Monday and
/// Tuesday".
fn ranges(values: &[u32], name: impl Fn(u32) -> String) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < values.len() {
        let mut j = i;
        while j + 1 < values.len() && values[j + 1] == values[j] + 1 {
            j += 1;
        }
        if j - i >= 2 {
            out.push(format!("{} through {}", name(values[i]), name(values[j])));
        } else {
            out.extend((i..=j).map(|k| name(values[k])));
        }
        i = j + 1;
    }
    out
}

fn full_day(d: u32) -> String {
    [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ][d as usize]
        .to_owned()
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// "a", "a and b", "a, b and c".
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        _ => format!(
            "{} and {}",
            items[..items.len() - 1].join(", "),
            items[items.len() - 1]
        ),
    }
}

/// Parses one cron field into the sorted values it allows.
fn parse_field(
    text: &str,
    min: u32,
    max: u32,
    names: &[&str],
    label: &str,
) -> Result<Vec<u32>, ToolError> {
    let mut allowed = vec![false; (max - min + 1) as usize];

    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(bad(label, text, "it has an empty entry between commas"));
        }

        let (spec, step) = match part.split_once('/') {
            Some((spec, step)) => {
                let step: u32 = step
                    .parse()
                    .map_err(|_| bad(label, text, &format!("\"{step}\" is not a step number")))?;
                if step == 0 {
                    return Err(bad(label, text, "a step of 0 would never advance"));
                }
                (spec, step)
            }
            None => (part, 1),
        };

        let (from, to) = if spec == "*" {
            (min, max)
        } else if let Some((a, b)) = spec.split_once('-') {
            (
                value(a, min, max, names, label, text)?,
                value(b, min, max, names, label, text)?,
            )
        } else {
            let single = value(spec, min, max, names, label, text)?;
            // `5/10` means "from 5, every 10" — a bare value with a step runs
            // to the end of the range rather than matching only itself.
            if step > 1 {
                (single, max)
            } else {
                (single, single)
            }
        };

        if from > to {
            return Err(bad(
                label,
                text,
                &format!("the range {from}-{to} runs backwards"),
            ));
        }
        let mut v = from;
        while v <= to {
            allowed[(v - min) as usize] = true;
            v += step;
        }
    }

    let values: Vec<u32> = allowed
        .iter()
        .enumerate()
        .filter(|(_, on)| **on)
        .map(|(i, _)| i as u32 + min)
        .collect();
    if values.is_empty() {
        return Err(bad(label, text, "it allows no values at all"));
    }
    Ok(values)
}

fn value(
    token: &str,
    min: u32,
    max: u32,
    names: &[&str],
    label: &str,
    field: &str,
) -> Result<u32, ToolError> {
    let token = token.trim();
    if let Some(index) = names.iter().position(|n| n.eq_ignore_ascii_case(token)) {
        // Month names start at 1, weekday names at 0.
        return Ok(index as u32 + min);
    }
    let number: u32 = token.parse().map_err(|_| {
        bad(
            label,
            field,
            &format!("\"{token}\" is not a number or a name"),
        )
    })?;
    // Sunday is both 0 and 7 in every cron implementation worth matching.
    let number = if max == 6 && number == 7 { 0 } else { number };
    if number < min || number > max {
        return Err(bad(
            label,
            field,
            &format!("{number} is outside {min}-{max}"),
        ));
    }
    Ok(number)
}

fn bad(label: &str, field: &str, why: &str) -> ToolError {
    ToolError::new(format!("{label} field \"{field}\" is not valid: {why}."))
}

// ----------------------------------------------------------- civil calendar
//
// Days are counted from 1970-01-01. These are Howard Hinnant's algorithms,
// exact for every proleptic Gregorian date, and the reason this tool needs no
// date library.

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 0 = Sunday, matching cron's numbering.
fn weekday(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-08-19 12:34:00 UTC, a Wednesday. Every expectation below is
    /// relative to this, so the tests never depend on the real clock.
    const NOW: i64 = 1_787_142_840;

    fn opts(fields: [&str; 5], runs: i64) -> Options {
        let mut o = Options::from_specs(OPTIONS);
        for (id, value) in ["minute", "hour", "dom", "month", "dow"].iter().zip(fields) {
            o.set(
                match *id {
                    "minute" => "minute",
                    "hour" => "hour",
                    "dom" => "dom",
                    "month" => "month",
                    _ => "dow",
                },
                OptionValue::Text(value.to_owned()),
            );
        }
        o.set("runs", OptionValue::Number(runs));
        o
    }

    fn runs(fields: [&str; 5], count: i64) -> Vec<String> {
        let out = report(&opts(fields, count), NOW).unwrap();
        out.lines()
            .skip_while(|l| !l.starts_with("NEXT"))
            .skip(1)
            .map(|l| l.trim().to_owned())
            .collect()
    }

    fn describe(fields: [&str; 5]) -> String {
        let out = report(&opts(fields, 1), NOW).unwrap();
        out.lines().nth(2).unwrap().to_owned()
    }

    #[test]
    fn the_epoch_and_a_leap_day_convert_correctly() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(weekday(0), 4, "1970-01-01 was a Thursday");
        // 2024 was a leap year; 2100 is not.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn every_day_of_one_year_round_trips_through_the_calendar() {
        // Guards the arithmetic that replaces a date library.
        let mut expected = (2024, 1, 1);
        let start = 19_723; // 2024-01-01
        for offset in 0..366 {
            assert_eq!(civil_from_days(start + offset), expected, "day {offset}");
            let (y, m, d) = expected;
            let last = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m - 1) as usize];
            expected = if d == last {
                (y, m + 1, 1)
            } else {
                (y, m, d + 1)
            };
        }
    }

    #[test]
    fn a_daily_time_lists_consecutive_days() {
        let out = runs(["30", "9", "*", "*", "*"], 3);
        assert_eq!(
            out,
            vec![
                "Thu  2026-08-20  09:30",
                "Fri  2026-08-21  09:30",
                "Sat  2026-08-22  09:30",
            ]
        );
    }

    #[test]
    fn a_time_still_to_come_today_is_included() {
        // NOW is 12:34, so 13:00 today is next.
        assert_eq!(
            runs(["0", "13", "*", "*", "*"], 1),
            vec!["Wed  2026-08-19  13:00"]
        );
    }

    #[test]
    fn a_time_already_past_today_waits_for_tomorrow() {
        assert_eq!(
            runs(["0", "9", "*", "*", "*"], 1),
            vec!["Thu  2026-08-20  09:00"]
        );
    }

    #[test]
    fn weekday_names_skip_the_weekend() {
        let out = runs(["30", "9", "*", "*", "MON-FRI"], 4);
        assert_eq!(
            out,
            vec![
                "Thu  2026-08-20  09:30",
                "Fri  2026-08-21  09:30",
                "Mon  2026-08-24  09:30",
                "Tue  2026-08-25  09:30",
            ],
            "Saturday and Sunday must be skipped"
        );
    }

    #[test]
    fn steps_expand_within_the_hour() {
        let out = runs(["*/15", "13", "*", "*", "*"], 4);
        assert_eq!(
            out,
            vec![
                "Wed  2026-08-19  13:00",
                "Wed  2026-08-19  13:15",
                "Wed  2026-08-19  13:30",
                "Wed  2026-08-19  13:45",
            ]
        );
    }

    #[test]
    fn a_bare_value_with_a_step_runs_to_the_end_of_the_range() {
        // `5/20` is "from 5, every 20", not "only 5".
        let out = runs(["5/20", "13", "*", "*", "*"], 3);
        assert_eq!(
            out,
            vec![
                "Wed  2026-08-19  13:05",
                "Wed  2026-08-19  13:25",
                "Wed  2026-08-19  13:45",
            ]
        );
    }

    #[test]
    fn lists_are_honoured_and_sorted() {
        let out = runs(["0", "14,8,20", "*", "*", "*"], 3);
        assert_eq!(
            out,
            vec![
                "Wed  2026-08-19  14:00",
                "Wed  2026-08-19  20:00",
                "Thu  2026-08-20  08:00",
            ]
        );
    }

    #[test]
    fn day_of_month_and_weekday_together_match_either() {
        // Vixie's rule: the 1st *or* any Monday.
        let out = runs(["0", "0", "1", "*", "MON"], 4);
        assert_eq!(
            out,
            vec![
                "Mon  2026-08-24  00:00",
                "Mon  2026-08-31  00:00",
                "Tue  2026-09-01  00:00",
                "Mon  2026-09-07  00:00",
            ]
        );
    }

    #[test]
    fn a_restricted_day_alone_must_match() {
        let out = runs(["0", "0", "1", "*", "*"], 2);
        assert_eq!(
            out,
            vec!["Tue  2026-09-01  00:00", "Thu  2026-10-01  00:00"]
        );
    }

    #[test]
    fn month_names_and_a_yearly_schedule_work() {
        let out = runs(["0", "0", "25", "DEC", "*"], 2);
        assert_eq!(
            out,
            vec!["Fri  2026-12-25  00:00", "Sat  2027-12-25  00:00"]
        );
    }

    #[test]
    fn a_leap_day_schedule_finds_the_next_leap_year() {
        let out = runs(["0", "12", "29", "FEB", "*"], 1);
        assert_eq!(out, vec!["Tue  2028-02-29  12:00"]);
    }

    #[test]
    fn sunday_is_both_zero_and_seven() {
        assert_eq!(
            runs(["0", "0", "*", "*", "0"], 1),
            runs(["0", "0", "*", "*", "7"], 1)
        );
    }

    #[test]
    fn an_impossible_date_terminates_instead_of_hanging() {
        let out = report(&opts(["0", "0", "30", "FEB", "*"], 5), NOW).unwrap();
        assert!(out.contains("Never"), "{out}");
    }

    #[test]
    fn empty_fields_mean_no_constraint() {
        let mut o = Options::from_specs(OPTIONS);
        for id in ["minute", "hour", "dom", "month", "dow"] {
            o.set(id, OptionValue::Text(String::new()));
        }
        o.set("runs", OptionValue::Number(2));
        let out = report(&o, NOW).unwrap();
        assert!(out.starts_with("* * * * *"), "{out}");
        assert!(out.contains("Every minute"), "{out}");
    }

    #[test]
    fn bad_fields_say_which_field_and_why() {
        let cases = [
            (["99", "*", "*", "*", "*"], "Minute", "outside 0-59"),
            (
                ["*", "*", "*", "SMARCH", "*"],
                "Month",
                "not a number or a name",
            ),
            (["*/0", "*", "*", "*", "*"], "Minute", "never advance"),
            (["5-1", "*", "*", "*", "*"], "Minute", "backwards"),
            (["1,,2", "*", "*", "*", "*"], "Minute", "empty entry"),
        ];
        for (fields, field, why) in cases {
            let err = report(&opts(fields, 1), NOW).unwrap_err();
            assert!(err.0.contains(field) && err.0.contains(why), "{}", err.0);
        }
    }

    #[test]
    fn descriptions_read_like_english() {
        assert_eq!(
            describe(["30", "9", "*", "*", "MON-FRI"]),
            "At 09:30, on Monday through Friday."
        );
        assert_eq!(describe(["*", "*", "*", "*", "*"]), "Every minute.");
        assert_eq!(
            describe(["0", "0", "25", "DEC", "*"]),
            "At 00:00, on the 25th of the month, in December."
        );
    }

    #[test]
    fn contiguous_values_describe_themselves_as_a_range() {
        assert_eq!(
            ranges(&[1, 2, 3, 4, 5], full_day),
            vec!["Monday through Friday"]
        );
        // Two in a row read better listed than ranged.
        assert_eq!(ranges(&[1, 2], full_day), vec!["Monday", "Tuesday"]);
        // Separate runs stay separate.
        assert_eq!(
            ranges(&[0, 1, 2, 6], full_day),
            vec!["Sunday through Tuesday", "Saturday"]
        );
    }

    #[test]
    fn a_description_warns_about_the_either_or_day_rule() {
        assert!(describe(["0", "0", "1", "*", "MON"]).contains("either condition is enough"));
    }

    #[test]
    fn the_expression_is_echoed_so_it_can_be_copied() {
        let out = report(&opts(["30", "9", "*", "*", "MON-FRI"], 1), NOW).unwrap();
        assert_eq!(out.lines().next().unwrap(), "30 9 * * MON-FRI");
    }
}
