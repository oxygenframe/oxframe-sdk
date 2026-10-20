//! Draw a logo on, one path at a time, then recolor one part.

use oxframe::prelude::*;

const LOGO: &[u8] =
    br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="60" viewBox="0 0 120 60">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#e13238"/>
      <stop offset="1" stop-color="#68abdf"/>
    </linearGradient>
  </defs>
  <g id="mark" transform="translate(16,10)">
    <rect id="bar" x="0" y="0" width="36" height="36" rx="6" fill="url(#g)"/>
    <circle id="dot" cx="64" cy="18" r="10" fill="#f5c242"/>
  </g>
</svg>"##;

#[oxframe::scene]
pub async fn logo(view: View) {
    let logo = view.add(Svg::new().src(LOGO).width(640.0));
    stagger(0.15, logo.paths(), |p| p.draw_on()).await;
    // The id comes from the file, so one part can be recolored on its own.
    logo.find("dot").fill().to(color!("#e13238"), 0.6).await;
    wait_for(0.4).await;
}

pub fn project() -> Project {
    Project::new("svg").scene(logo())
}
