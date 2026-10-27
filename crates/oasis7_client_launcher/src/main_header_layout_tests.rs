use super::*;

#[test]
fn narrow_header_keeps_status_items_readable_and_bounded() {
    let ctx = egui::Context::default();
    configure_egui_visuals(&ctx);
    let mut app = ClientLauncherApp::default();
    app.config.agent_decision_source = "provider_backed".into();
    let mut header_height = 0.0;
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(390.0, 844.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                header_height = ui
                    .scope(|ui| app.render_launcher_header(ui))
                    .response
                    .rect
                    .height();
            });
        },
    );
    assert!(
        header_height < 180.0,
        "390-point header grew to {header_height}"
    );
}

#[test]
fn status_chip_wraps_as_one_nonclickable_item_at_narrow_line_end() {
    let ctx = egui::Context::default();
    let mut rectangles = Vec::new();
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(390.0, 844.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.allocate_exact_size(egui::vec2(310.0, 20.0), egui::Sense::hover());
                    for label in ["Provider", "Industry task executor"] {
                        let response = ClientLauncherApp::render_status_chip(
                            ui,
                            label,
                            "Unknown",
                            egui::Color32::GRAY,
                        );
                        assert!(!response.sense.senses_click());
                        rectangles.push(response.rect);
                    }
                });
            });
        },
    );
    assert_eq!(rectangles.len(), 2);
    for rect in &rectangles {
        assert!(
            rect.height() < 30.0,
            "status became a vertical column: {rect:?}"
        );
        assert!(rect.width() > 80.0, "status was squeezed: {rect:?}");
        assert!(
            rect.right() <= 390.0,
            "status clipped horizontally: {rect:?}"
        );
    }
    assert!(
        rectangles[0].top() >= 28.0,
        "whole chip did not wrap: {:?}",
        rectangles[0]
    );
}

#[test]
fn cjk_font_parses_and_tessellates_with_static_and_owned_font_data() {
    for font_data in [
        egui::FontData::from_static(EGUI_CJK_FONT_BYTES),
        egui::FontData::from_owned(EGUI_CJK_FONT_BYTES.to_vec()),
    ] {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        install_cjk_font(&mut fonts, OASIS7_CJK_FONT_NAME.to_string(), font_data);
        ctx.set_fonts(fonts);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let font_id = egui::FontId::proportional(16.0);
            assert!(ui.fonts_mut(|fonts| fonts.has_glyphs(&font_id, "启动器 Oasis7")));
            ui.label(egui::RichText::new("启动器 Oasis7").font(font_id));
        });
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        assert!(primitives.iter().any(|primitive| {
            matches!(&primitive.primitive, egui::epaint::Primitive::Mesh(mesh) if !mesh.vertices.is_empty())
        }));
    }
}
