//! Current reader against bytes emitted by the original v7–v10 producer commits.

use world_runtime::Runtime;

type HistoricalCase = (
    &'static str,
    &'static [u8],
    &'static [u8],
    u64,
    usize,
    usize,
);

const CASES: [HistoricalCase; 4] = [
    (
        "v7",
        include_bytes!("fixtures/qa1/v7.json"),
        include_bytes!("fixtures/qa1/v7-after3.json"),
        1,
        1,
        0,
    ),
    (
        "v8",
        include_bytes!("fixtures/qa1/v8.json"),
        include_bytes!("fixtures/qa1/v8-after3.json"),
        0,
        1,
        0,
    ),
    (
        "v9",
        include_bytes!("fixtures/qa1/v9.json"),
        include_bytes!("fixtures/qa1/v9-after3.json"),
        0,
        1,
        0,
    ),
    (
        "v10",
        include_bytes!("fixtures/qa1/v10.json"),
        include_bytes!("fixtures/qa1/v10-after3.json"),
        1,
        2,
        1,
    ),
];

#[test]
fn authentic_v7_to_v10_saves_load_continue_and_resave() {
    for (version, bytes, historical_after3, tick, fields, sources) in CASES {
        let original = Runtime::load_bytes(bytes)
            .unwrap_or_else(|error| panic!("{version} load failed: {error}"));
        assert_eq!(original.time().ticks(), tick, "{version}");
        assert_eq!(
            original.world().field_instances().len(),
            fields,
            "{version}"
        );
        assert_eq!(original.world().sources().len(), sources, "{version}");
        assert_eq!(
            original.world().field_instances()[0].id().value(),
            0,
            "{version} identity"
        );
        assert!(
            original
                .world()
                .field_instances()
                .iter()
                .all(|field| original.world().field_graph(field.id()).is_some())
        );
        let resaved = original.save_bytes().unwrap();
        let loaded_again = Runtime::load_bytes(&resaved).unwrap();
        assert_eq!(loaded_again, original, "{version} resave");
        let mut left = original;
        let mut right = loaded_again;
        for _ in 0..3 {
            left.tick().unwrap();
            right.tick().unwrap();
            assert_eq!(left, right, "{version} continuation");
        }
        let producer_continuation = Runtime::load_bytes(historical_after3).unwrap();
        assert_eq!(
            left, producer_continuation,
            "{version} historical continuation drift"
        );
        let mut downgraded: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        downgraded["format_version"] = version[1..]
            .parse::<u32>()
            .unwrap()
            .saturating_sub(1)
            .into();
        assert!(
            Runtime::load_bytes(&serde_json::to_vec(&downgraded).unwrap()).is_err(),
            "{version} unsupported downgrade"
        );
        let mut damaged = bytes.to_vec();
        damaged.truncate(damaged.len() / 2);
        let before = left.clone();
        assert!(
            left.load_into(&damaged).is_err(),
            "{version} truncated save"
        );
        assert_eq!(left, before, "{version} failed load changed running state");
    }
}
