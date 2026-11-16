//! A stored snapshot or action reads back as the value it was, and a stored
//! value no saga could save never reads back at all.
use std::format;
use std::string::{String, ToString};

use periscope_machines::scoped;

use super::reachable::{SCOPE, walk};
use super::*;

fn stored<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("plain data serializes")
}

fn read<T: serde::de::DeserializeOwned>(json: &str) -> Option<T> {
    serde_json::from_str(json).ok()
}

#[test]
fn every_reachable_snapshot_and_pending_action_round_trips() {
    let sagas = walk::<3>();
    assert!(sagas.len() > 100, "only {} states walked", sagas.len());
    for saga in sagas {
        let json = stored(&saga.snapshot());
        let snapshot: Option<Snapshot> = read(&json);
        assert_eq!(snapshot, Some(saga.snapshot()), "{json}");
        let restored = snapshot.and_then(|snapshot| Saga::<3>::restored(snapshot, SCOPE));
        assert_eq!(
            restored,
            Saga::<3>::restored(saga.snapshot(), SCOPE),
            "{json}"
        );
        if let Some(action) = saga.pending() {
            let mut resumed = saga.clone();
            let json = stored(&action);
            let read_back: Option<Action> = read(&json);
            assert_eq!(read_back, Some(action), "{json}");
            if let Some(action) = read_back {
                assert_eq!(
                    resumed.resolve(action, Outcome::Success),
                    Ok(Resolution::Resolved)
                );
            }
        }
    }
}

#[test]
fn values_keep_their_plain_shapes() {
    let action = Action::new(
        ActionKey::new(ID, 1, Kind::Compensate),
        attempt(scoped(2, 5)),
    );
    assert_eq!(
        stored(&action),
        r#"{"key":{"saga":7,"step":1,"kind":"Compensate"},"attempt":562949953421317}"#
    );
    let snapshot = saved(
        Progress::Compensating(0),
        2,
        Some((0, Kind::Forward, Outcome::Success, 1)),
    );
    let json = stored(&snapshot);
    assert_eq!(
        json,
        concat!(
            r#"{"id":7,"scope":0,"progress":{"Compensating":0},"issued":2,"settled":"#,
            r#"{"action":{"key":{"saga":7,"step":0,"kind":"Forward"},"attempt":1},"#,
            r#""outcome":"Success"}}"#
        )
    );
    assert_eq!(read::<Progress>(r#""Complete""#), Some(Progress::Complete));
    assert_eq!(
        read::<SagaId>("18446744073709551615"),
        Some(SagaId::from_raw(u64::MAX))
    );
}

#[test]
fn a_stored_value_no_saga_saves_is_refused() {
    for raw in ["0", &scoped(3, 0).to_string()] {
        assert_eq!(read::<AttemptId>(raw), None, "{raw}");
    }
    assert_eq!(read::<AttemptId>("1"), AttemptId::from_raw(1));
    let settled = |scope: u16, issued: u64, saga: u64, attempt: u64| {
        format!(
            concat!(
                r#"{{"id":7,"scope":{},"progress":{{"Forward":1}},"issued":{},"settled":"#,
                r#"{{"action":{{"key":{{"saga":{},"step":0,"kind":"Forward"}},"attempt":{}}},"#,
                r#""outcome":"Success"}}}}"#
            ),
            scope, issued, saga, attempt
        )
    };
    assert!(read::<Snapshot>(&settled(0, 1, 7, 1)).is_some());
    let refused = [
        // Issued past 2^48 - 1.
        String::from(concat!(
            r#"{"id":7,"scope":0,"progress":{"Forward":0},"#,
            r#""issued":281474976710656,"settled":null}"#
        )),
        // A settlement of another saga.
        settled(0, 1, 8, 1),
        // A settled attempt past the issued count.
        settled(0, 1, 7, 2),
        // A settled attempt of another scope.
        settled(1, 1, 7, 1),
        // A settled attempt of sequence zero.
        settled(0, 1, 7, 0),
        // A field no snapshot has.
        String::from(concat!(
            r#"{"id":7,"scope":0,"progress":{"Forward":0},"#,
            r#""issued":0,"settled":null,"pending":null}"#
        )),
    ];
    for json in refused {
        assert_eq!(read::<Snapshot>(&json), None, "{json}");
    }
    // Sound per field, but no saga of three steps saves it: `restored` judges.
    let json = r#"{"id":7,"scope":0,"progress":"Complete","issued":1,"settled":null}"#;
    let snapshot = read::<Snapshot>(json);
    assert!(snapshot.is_some());
    assert_eq!(snapshot.and_then(|s| Saga::<3>::restored(s, 0)), None);
}
