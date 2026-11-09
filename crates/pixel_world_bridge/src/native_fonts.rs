//! Fixed local glyph fallback for every native Text2d producer.
use bevy::prelude::*;
use bevy::text::{FontCx, Text2dUpdateSystems, load_font_assets_into_font_collection};
use fontique::Script;

const CJK_FONT: &[u8] = include_bytes!("../assets/fonts/noto-sans-sc/NotoSansSC-Regular.otf");
const FAMILY: &str = "Noto Sans SC";

/// Ship copyright and the complete OFL with the embedded font in every WASM
/// backend, including consumers that distribute the runtime without repo files.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn native_font_notices() -> String {
    concat!(
        "Noto Sans SC Regular 2.004 — Copyright © 2014–2021 Adobe (http://www.adobe.com/).\n\n",
        include_str!("../assets/fonts/noto-sans-sc/OFL.txt")
    )
    .to_owned()
}

#[derive(Resource)]
struct NativeFallbackFont {
    // Keep the asset alive while the renderer owns its text collection.
    _font: Handle<Font>,
}

pub(super) fn install(app: &mut App) {
    let font = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(CJK_FONT.to_vec()));
    app.insert_resource(NativeFallbackFont { _font: font });
    // Reapply after Bevy rebuilds its collection (including asset removal).
    app.add_systems(
        PostUpdate,
        configure_fallback
            .after(load_font_assets_into_font_collection)
            .before(Text2dUpdateSystems),
    );
}

fn configure_fallback(mut fonts: ResMut<FontCx>) {
    let Some(family) = fonts.collection.family_id(FAMILY) else {
        return;
    };
    // Fira stays the requested font. These are fallbacks for missing glyphs,
    // including isolated full-width punctuation; the full upstream face is retained.
    for script in ["Hani", "Bopo", "Hira", "Kana", "Zyyy"] {
        let key = Script::from_str_unchecked(script);
        let already_configured = fonts.collection.fallback_families(key).eq([family]);
        if !already_configured {
            fonts.collection.set_fallbacks(key, [family].into_iter());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_font_registers_complete_cjk_fallback_without_replacing_primary() {
        let mut app = App::new();
        app.init_resource::<Assets<Font>>()
            .init_resource::<FontCx>();
        let primary = app.world_mut().resource_mut::<Assets<Font>>().insert(
            bevy::asset::AssetId::default(),
            Font::from_bytes(bevy::text::DEFAULT_FONT_DATA.to_vec()),
        );
        assert!(primary.is_ok());
        install(&mut app);
        app.add_systems(PostUpdate, load_font_assets_into_font_collection);
        app.update();
        let mut fonts = app.world_mut().resource_mut::<FontCx>();
        let family = fonts
            .collection
            .family_id(FAMILY)
            .expect("embedded regional face");
        let info = fonts.collection.family(family).unwrap();
        let cmap = info.fonts()[0].charmap_index().charmap(CJK_FONT).unwrap();
        let covered = (0x4e00u32..=0x9fff)
            .filter(|cp| cmap.map(*cp).is_some())
            .count();
        assert!(
            covered > 20_000,
            "retain upstream Han coverage, not fixture-only glyphs"
        );
        for ch in "独立观察目标一号长中文名称阻塞状态资源位置仓库工厂，。：！？−123ABC".chars()
        {
            assert!(cmap.map(ch).is_some(), "missing native label glyph: {ch}");
        }
        for script in ["Hani", "Bopo", "Hira", "Kana", "Zyyy"] {
            assert!(
                fonts
                    .collection
                    .fallback_families(Script::from_str_unchecked(script))
                    .any(|id| id == family)
            );
        }
        assert_eq!(
            app.world()
                .resource::<Assets<Font>>()
                .get(bevy::asset::AssetId::default())
                .unwrap()
                .data
                .as_ref(),
            bevy::text::DEFAULT_FONT_DATA
        );
    }

    #[test]
    fn removing_an_unrelated_font_restores_cjk_fallback_after_collection_rebuild() {
        let mut app = App::new();
        app.init_resource::<Assets<Font>>()
            .init_resource::<FontCx>();
        let temporary = app
            .world_mut()
            .resource_mut::<Assets<Font>>()
            .add(Font::from_bytes(bevy::text::DEFAULT_FONT_DATA.to_vec()));
        install(&mut app);
        app.add_systems(PostUpdate, load_font_assets_into_font_collection);
        app.update();
        app.world_mut()
            .resource_mut::<Assets<Font>>()
            .remove(temporary.id());
        app.update();
        let mut fonts = app.world_mut().resource_mut::<FontCx>();
        let family = fonts.collection.family_id(FAMILY).unwrap();
        assert!(
            fonts
                .collection
                .fallback_families(Script::from_bytes(*b"Hani"))
                .any(|id| id == family)
        );
        let notice = native_font_notices();
        assert!(notice.contains("Copyright © 2014–2021 Adobe"));
        assert!(notice.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert!(notice.contains("PERMISSION & CONDITIONS"));
    }
}
