use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fluxa_core::bench_targets::player_source_sidebar_plan_json;
use serde_json::json;

fn request_with_streams(count: usize) -> String {
    let addons = ["Torrentio", "Cinemeta Addon", "Public Domain", "USA TV"];
    let streams: Vec<_> = (0..count)
        .map(|i| {
            json!({
                "addonName": addons[i % addons.len()],
                "title": format!("Stream {i}"),
                "name": format!("Source {i}"),
                "quality": "1080p",
            })
        })
        .collect();
    json!({
        "streams": streams,
        "currentStreamIndex": 0,
        "availableAddons": addons,
        "selectedAddon": Option::<String>::None,
    })
    .to_string()
}

fn source_sidebar_plan_500_streams(c: &mut Criterion) {
    let request = request_with_streams(500);
    c.bench_function("source_sidebar_plan_500_streams", |b| {
        b.iter(|| black_box(player_source_sidebar_plan_json(black_box(&request))))
    });
}

criterion_group!(benches, source_sidebar_plan_500_streams);
criterion_main!(benches);
