//! Rain and a spray that come out the same wherever you seek.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn spray(view: View) {
    let _rain = view.add(
        Custom::new(|ctx, t| {
            let mut rng = ctx.rng(42);
            let drops = (48.0 * ctx.quality()).round() as i32;
            for _ in 0..drops {
                let x = rng.range(-960.0..960.0);
                let y = (rng.range(0.0..1080.0) + t * 420.0) % 1080.0 - 540.0;
                ctx.line((x, y), (x, y + 16.0)).stroke(Muted, 2.0);
            }
        })
        .fullscreen()
        .name("rain"),
    );
    let spray = view.add(
        Particles::new()
            .seed(7)
            .gravity(900.0)
            .fill(Surface)
            .toward_camera()
            .name("spray"),
    );
    spray.burst((0.0, 40.0), 48).await;
    wait_for(1.2).await;
}

pub fn project() -> Project {
    Project::new("spray").seed(7).scene(spray())
}
