use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::game::notes::{SCREEN_H, screen_w};
use crate::skin::AtlasRegion;
use crate::ui::{draw_menu_background, draw_osu_circle, draw_menu_tabs};

const SETTINGS: [u8; 4] = [85, 85, 85, 255];
const PLAY: [u8; 4] = [97, 70, 197, 255];
const EDIT: [u8; 4] = [228, 173, 59, 255];
const BROWSE: [u8; 4] = [173, 203, 63, 255];
const EXIT: [u8; 4] = [219, 69, 151, 255];

pub fn render(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    cover_region: Option<&AtlasRegion>,
    selected: usize,
    hovered: Option<usize>,
    logo: Option<&AtlasRegion>,
) {
    let w = screen_w();
    draw_menu_background(quad, cover_region);

    let tabs: &[(&str, [u8; 4])] = &[
        ("Settings", SETTINGS), ("Play", PLAY), ("Edit", EDIT),
        ("Browse", BROWSE), ("Exit", EXIT),
    ];
    let circle_r = w / 3.0 / 2.0;
    let circle_cx = circle_r + w * 0.1;
    let circle_cy = SCREEN_H / 2.0;

    let splash_r = SCREEN_H * 0.45;
    let menu_font = 192.0 * circle_r / splash_r;
    draw_menu_tabs(quad, text, tabs, selected, hovered, circle_cx, circle_r);
    draw_osu_circle(quad, text, circle_cx, circle_cy, circle_r, Some("Oxidized Mania"), menu_font, logo);
}
