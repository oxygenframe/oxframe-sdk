//! Paint and compositing: gradients, dashes, group opacity, clipping, blend
//! modes, blur, shadows and glow — all animatable.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn effects(view: View) {
    // A gradient card with marching-ants border and a soft shadow.
    let card = view.add(
        Rect::new()
            .width(520.0)
            .height(300.0)
            .radius(28.0)
            .position((-400.0, -120.0))
            .fill_gradient(
                Gradient::linear((-260.0, -150.0), (260.0, 150.0))
                    .stops([color!("#e13238"), color!("#f5c242")]),
            )
            .stroke(Color::WHITE)
            .line_width(5.0)
            .dash(vec![18.0, 12.0])
            .shadow(Color::BLACK.with_alpha(0.6), 18.0, (0.0, 16.0)),
    );
    spawn(async move {
        card.dash_offset()
            .to(-300.0, 6.0)
            .ease(presets::LINEAR)
            .await;
    });

    // Clipped content: a stripe sweeping inside a circle.
    let lens = view.add(
        Circle::new()
            .size(300.0)
            .position((400.0, -120.0))
            .fill(color!("#1b1d22"))
            .clip(true),
    );
    let stripe = lens.add(
        Rect::new()
            .width(80.0)
            .height(400.0)
            .x(-220.0)
            .rotation(20.0)
            .fill(color!("#68abdf")),
    );

    // Glow that breathes, blur that focuses.
    let orb = view.add(
        Circle::new()
            .size(120.0)
            .position((-300.0, 260.0))
            .fill(color!("#68abdf"))
            .glow(color!("#68abdf"), 4.0),
    );
    let title = view.add(
        Txt::new()
            .text("in focus")
            .font_size(80.0)
            .weight(800.0)
            .position((200.0, 260.0))
            .blur(14.0),
    );

    all((
        stripe.x().to(220.0, 2.0),
        orb.shadow_blur().to(30.0, 2.0),
        title.blur().to(0.0, 2.0),
    ))
    .await;

    // Group opacity: the overlap of the two circles never gets darker.
    let pair = view.add(
        Group::new()
            .y(260.0)
            .x(-40.0)
            .opacity(1.0)
            .child(Circle::new().size(140.0).x(-50.0).fill(color!("#2ecc71")))
            .child(Circle::new().size(140.0).x(50.0).fill(color!("#2ecc71"))),
    );
    pair.opacity().to(0.4, 1.0).await;

    // Multiply blend over the card.
    let mul = view.add(
        Circle::new()
            .size(200.0)
            .position((-400.0, -120.0))
            .fill(color!("#68abdf"))
            .blend(BlendMode::Multiply)
            .scale(0.0),
    );
    mul.scale()
        .to(Vec2::new(1.0, 1.0), 0.8)
        .ease(presets::OUT_BACK)
        .await;
    wait_for(1.0).await;
}

pub fn project() -> Project {
    Project::new("effects").scene(effects())
}
