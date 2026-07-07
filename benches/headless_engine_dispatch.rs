use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use fluxa_core::bench_targets::{
    create_headless_engine, destroy_headless_engine, headless_engine_dispatch_json,
};
use serde_json::json;

fn large_initial_state() -> String {
    let streams: Vec<_> = (0..500)
        .map(|i| {
            json!({
                "id": i,
                "name": format!("Stream {i}"),
                "url": format!("https://cdn.example/{i}.mkv"),
            })
        })
        .collect();
    json!({
        "detail": {
            "id": "tt1",
            "contentType": "movie",
            "streams": streams,
        }
    })
    .to_string()
}

fn navigation_dispatch_on_large_state(c: &mut Criterion) {
    let initial = large_initial_state();
    c.bench_function("navigation_dispatch_on_large_state", |b| {
        b.iter_batched(
            || create_headless_engine(&initial),
            |handle| {
                let result = headless_engine_dispatch_json(
                    handle,
                    r#"{"type":"navigationRequested","route":"home","params":null}"#,
                );
                destroy_headless_engine(handle);
                black_box(result)
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, navigation_dispatch_on_large_state);
criterion_main!(benches);
