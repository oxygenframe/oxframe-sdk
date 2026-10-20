//! Write a function in steps: unchanged tokens slide, new ones appear.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn steps(view: View) {
    let code = view.add(
        Code::new()
            .language("rust")
            .font_size(42.0)
            .code("fn double(x: i32) -> i32 {\n}\n"),
    );
    wait_for(0.3).await;
    // The body appears on its own line.
    code.insert(2, 0, "    x\n", 0.7).await;
    code.focus_lines(2, 2, 0.3).await;
    // `x` slides down as the new statement pushes it.
    code.insert(2, 4, "let y = x * 2;\n    ", 0.8).flash().await;
    code.replace(((3, 4), (3, 5)), "y", 0.5).await;
    code.focus_lines(2, 3, 0.4).await;
    wait_for(0.4).await;
}

pub fn project() -> Project {
    Project::new("code_steps").scene(steps())
}
