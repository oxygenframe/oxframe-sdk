//! A table that changes on screen: a row slides in, the rows sort by a
//! column, and the slowest one leaves.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn ranking(view: View) {
    view.add(
        Txt::new()
            .text("Frame time at 1080p")
            .h1()
            .to_edge(Edge::Top),
    );
    let table = view.add(
        Table::new(["Renderer", "Backend", "Frame"])
            .row(["skia", "CPU", "12 ms"])
            .row(["vello", "GPU", "4 ms"])
            .row(["cairo", "CPU", "18 ms"])
            .row(["tiny-skia", "CPU", "9 ms"])
            .font_size(44.0)
            .row_height(92.0)
            .widths([300.0, 220.0, 200.0])
            .y(40.0),
    );
    wait_for(0.6).await;
    table.insert_row(2, ["wgpu", "GPU", "3 ms"]).await;
    wait_for(0.4).await;
    table.sort_by_column(2, 0.8).await;
    let fastest = table.row(0);
    fastest.background().fade_in().await;
    wait_for(0.4).await;
    let last = table.len() - 1;
    table.remove_row(last).await;
    wait_for(0.8).await;
}

pub fn project() -> Project {
    Project::new("table_sort").scene(ranking())
}
