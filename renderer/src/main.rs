use skia_safe::{
    gradient::{Colors, Gradient, Interpolation},
    shaders, Color, Data, EncodedImageFormat, Font, FontMgr, FontStyle, Paint, PaintStyle, Point,
    RRect, Rect, TileMode,
};

fn main() {
    // P0 spike：800×1280 设备面 + 圆角 card + 中文/数字 + 渐变条 + measureText。
    let mut surface = skia_safe::surfaces::raster_n32_premul((800, 1280)).expect("skia surface");
    let canvas = surface.canvas();
    canvas.clear(Color::from_rgb(0x14, 0x15, 0x19));

    let card = Rect::from_xywh(24.0, 24.0, 752.0, 320.0);
    let rr = RRect::new_rect_xy(card, 12.0, 12.0);

    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color(Color::from_rgb(0x1a, 0x1c, 0x22));
    canvas.draw_rrect(rr, &p);

    p.set_style(PaintStyle::Stroke);
    p.set_stroke_width(1.0);
    p.set_color(Color::from_rgb(0x2a, 0x2c, 0x34));
    canvas.draw_rrect(rr, &p);

    let mgr = FontMgr::new();
    let typeface = Data::from_filename("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc")
        .and_then(|d| mgr.new_from_data(d, 0))
        .or_else(|| mgr.match_family_style("WenQuanYi Zen Hei", FontStyle::normal()))
        .expect("CJK typeface");

    let mut text = Paint::default();
    text.set_anti_alias(true);
    text.set_color(Color::from_rgb(0xd6, 0xd7, 0xdc));

    let label_font = Font::new(typeface.clone(), 22.0);
    canvas.draw_str("室内温度", (56.0, 86.0), &label_font, &text);

    let value_font = Font::new(typeface, 64.0);
    let value = "17.4°C";
    let (w, _) = value_font.measure_text(value, Some(&text));
    canvas.draw_str(value, (56.0, 166.0), &value_font, &text);
    println!("measure_text({value}) = {w}");

    // 底部渐变条：验证 Gradient + RRect。
    let colors = [
        Color::from_rgb(0x5f, 0xc9, 0x8d).into(),
        Color::from_rgb(0xd8, 0xbb, 0x85).into(),
    ];
    let gc = Colors::new_evenly_spaced(&colors, TileMode::Clamp, None);
    let grad = Gradient::new(gc, Interpolation::default());
    let shader = shaders::linear_gradient(
        (Point::new(56.0, 292.0), Point::new(744.0, 292.0)),
        &grad,
        None,
    )
    .expect("gradient shader");
    let mut gp = Paint::default();
    gp.set_shader(shader);
    canvas.draw_rrect(
        RRect::new_rect_xy(Rect::from_xywh(56.0, 286.0, 688.0, 10.0), 5.0, 5.0),
        &gp,
    );

    let image = surface.image_snapshot();
    let png = image
        .encode_to_data(EncodedImageFormat::PNG)
        .expect("png encode");
    std::fs::write("/tmp/cs80-skia-card.png", png.as_bytes()).expect("write png");
    println!("wrote /tmp/cs80-skia-card.png");
}
