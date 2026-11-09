use ceh_store::periods::{Period, bounds, clipped_seconds, union_seconds};
use chrono::{DateTime, Utc};
fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
#[test]
fn madrid_days_follow_dst_not_fixed_twenty_four_hours() {
    for (input, start, end, hours) in [
        (
            "2026-03-29T12:00:00Z",
            "2026-03-28T23:00:00Z",
            "2026-03-29T22:00:00Z",
            23,
        ),
        (
            "2026-10-25T12:00:00Z",
            "2026-10-24T22:00:00Z",
            "2026-10-25T23:00:00Z",
            25,
        ),
        (
            "2026-10-29T23:30:00Z",
            "2026-10-29T23:00:00Z",
            "2026-10-30T23:00:00Z",
            24,
        ),
    ] {
        let b = bounds(Period::Today, at(input)).unwrap();
        assert_eq!(b.start, Some(at(start)));
        assert_eq!(b.end, Some(at(end)));
        assert_eq!((b.end.unwrap() - b.start.unwrap()).num_hours(), hours);
    }
}
#[test]
fn clips_intervals_and_does_not_invent_reversed_clock_duration() {
    let b = bounds(Period::Today, at("2026-03-29T12:00:00Z")).unwrap();
    assert_eq!(
        clipped_seconds(at("2026-03-28T22:00:00Z"), at("2026-03-29T23:00:00Z"), b),
        82800.0
    );
    assert_eq!(
        clipped_seconds(at("2026-03-29T12:00:00Z"), at("2026-03-29T11:00:00Z"), b),
        0.0
    );
}
#[test]
fn observed_sources_union_overlap_without_double_counting() {
    let b = bounds(Period::All, at("2026-10-29T12:00:00Z")).unwrap();
    let intervals = [
        (at("2026-10-29T10:00:00Z"), at("2026-10-29T11:00:00Z")),
        (at("2026-10-29T10:30:00Z"), at("2026-10-29T12:00:00Z")),
        (at("2026-10-29T10:45:00Z"), at("2026-10-29T10:50:00Z")),
    ];
    assert_eq!(union_seconds(&intervals, b), 7200.0);
}
