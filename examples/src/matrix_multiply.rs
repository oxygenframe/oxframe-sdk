//! Matrix multiplication step by step: a row of A meets a column of B and
//! their dot product fills one cell of the result.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn multiply(view: View) {
    view.add(
        Txt::new()
            .text("Multiplying matrices")
            .h1()
            .to_edge(Edge::Top),
    );
    let a_vals = [[1, 2], [3, 4]];
    let b_vals = [[5, 6], [7, 8]];
    let a = view.add(Matrix::new(a_vals).font_size(64.0).x(-560.0));
    view.add(Txt::new().text("×").h1().x(-310.0));
    let b = view.add(Matrix::new(b_vals).font_size(64.0).x(-60.0));
    view.add(Txt::new().text("=").h1().x(190.0));
    let c = view.add(
        Matrix::new([["?", "?"], ["?", "?"]])
            .font_size(64.0)
            .cell_width(80.0)
            .x(470.0),
    );
    for cell in (0..2).flat_map(|i| c.row(i)) {
        cell.fill().set(Muted);
    }
    let sum = view.add(Txt::new().text("").h2().y(280.0));
    let rows = [a.row_frame(0), a.row_frame(1)];
    let cols = [b.column_frame(0), b.column_frame(1)];
    wait_for(0.4).await;

    for i in 0..2 {
        for j in 0..2 {
            all((
                rows[i].opacity().to(1.0, 0.3),
                cols[j].opacity().to(1.0, 0.3),
            ))
            .await;
            let terms: Vec<String> = (0..2)
                .map(|k| format!("{}·{}", a_vals[i][k], b_vals[k][j]))
                .collect();
            let value: i32 = (0..2).map(|k| a_vals[i][k] * b_vals[k][j]).sum();
            sum.text().set(format!("{} = {value}", terms.join(" + ")));
            sum.opacity().set(0.0);
            sum.fade_in().await;
            let cell = c.cell(i, j);
            cell.text().set(value.to_string());
            cell.fill().set(Accent);
            wait_for(0.4).await;
            all((
                rows[i].opacity().to(0.0, 0.3),
                cols[j].opacity().to(0.0, 0.3),
                cell.fill().to(Text, 0.3),
            ))
            .await;
        }
    }
    sum.fade_out().await;
    wait_for(0.6).await;
}

pub fn project() -> Project {
    Project::new("matrix_multiply").scene(multiply())
}
