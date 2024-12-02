use bevy::{
    prelude::Entity,
    remote::{
        builtin_methods::{BrpQuery, BrpQueryFilter, BrpQueryParams, BRP_QUERY_METHOD},
        http::{DEFAULT_ADDR, DEFAULT_PORT},
        BrpRequest,
    },
    utils::HashMap,
};
use eframe::egui::{self, ViewportCommand};
use egui::{Color32, RichText};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// The response to a `bevy/query` request.
pub type BrpQueryResponse = Vec<BrpQueryRow>;

trait ToHashMap {
    fn to_hash_map(&self) -> HashMap<Entity, BrpQueryRow>;
}

impl ToHashMap for BrpQueryResponse {
    fn to_hash_map(&self) -> HashMap<Entity, BrpQueryRow> {
        self.into_iter().map(|el| (el.entity, el.clone())).collect()
    }
}

/// One query match result: a single entity paired with the requested components.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BrpQueryRow {
    /// The ID of the entity that matched.
    pub entity: Entity,

    /// The serialized values of the requested components.
    pub components: HashMap<String, Value>,

    /// The boolean-only containment query results.
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub has: HashMap<String, Value>,
}
enum Download {
    None,
    InProgress,
    Done,
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    #[serde(skip)] // This how you opt-out of serialization of a field
    download: Arc<Mutex<Download>>,
    #[serde(skip)]
    components: Arc<Mutex<HashMap<Entity, BrpQueryRow>>>,
    skip_empty_entities: bool,
    #[serde(skip)]
    error_info: Arc<Mutex<Option<String>>>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            download: Arc::new(Mutex::new(Download::None)),
            components: Arc::new(Mutex::new(HashMap::new())),
            skip_empty_entities: true,
            error_info: Arc::new(Mutex::new(None)),
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }

    fn draw_entity(
        &self,
        ui: &mut egui::Ui,
        entity: &Entity,
        components: &HashMap<Entity, BrpQueryRow>,
    ) {
        let Some(item) = components.get(entity) else {
            return;
        };
        let is_empty = item.components.len() == 0;
        if self.skip_empty_entities && is_empty {
            return;
        }
        let mut id = entity.to_string();
        if let Some(name) = item.components.get("bevy_core::name::Name") {
            let name = name
                .as_object()
                .map_or("NONE", |f| f.get("name").unwrap().as_str().unwrap());
            id += ": ";
            id += name;
        };
        egui::CollapsingHeader::new(RichText::new(id).strong())
            .default_open(false)
            .show(ui, |ui| {
                if let Some(children) = item
                    .components
                    .get("bevy_hierarchy::components::children::Children")
                {
                    let Some(array) = children.as_array() else {
                        return;
                    };
                    ui.heading("Children");
                    ui.separator();

                    let array: Vec<u64> = array.into_iter().map(|v| v.as_u64()).flatten().collect();
                    for el in array.iter() {
                        self.draw_entity(ui, &Entity::from_bits(*el), components);
                    }
                }

                ui.heading("Components");
                for (key, field) in item.components.iter() {
                    if key.eq("bevy_hierarchy::components::parent::Parent") {
                        continue;
                    }
                    if key.eq("bevy_hierarchy::components::children::Children") {
                        continue;
                    }

                    let Ok(json) = serde_json::to_string_pretty(field) else {
                        continue;
                    };
                    if json.eq("{}") {
                        ui.label(RichText::new(key).strong());
                        continue;
                    }
                    egui::CollapsingHeader::new(key)
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.label(json);
                        });
                }
            });
        ui.separator();
    }
}

impl eframe::App for TemplateApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array() // Make sure we don't paint anything behind the rounded corners
    }
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        custom_window_frame(ctx, "Bevy Inspector", |ui| {
            // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
            // For inspiration and more examples, go to https://emilk.github.io/egui

            // egui::TopBottomPanel::top("top_panel")
            //     .frame(egui::Frame::canvas(&ctx.style()))
            //     .show(ctx, |ui| {
            // ui.add_space(15.0);
            // ui.horizontal(|ui| {
            //     ui.add_space(8.0);
            //     ui.label(
            //         RichText::new("Bevy inspector")
            //             .strong()
            //             .size(25.0)
            //             .color(egui::Color32::from_rgb(230, 102, 1)),
            //     );
            // });
            ui.horizontal(|ui| {
                    let download_store = self.download.clone();
                    let is_downloading = matches!(&*download_store.lock().unwrap(), Download::InProgress);
                    ui.add_space(8.0);
                    ui.add_enabled_ui(!is_downloading, |ui|{

                        if ui.button("Fetch").clicked() {
                            let components = self.components.clone();
                            let error_info = self.error_info.clone();
                            *download_store.lock().unwrap() = Download::InProgress;
                            let egui_ctx = ctx.clone();
                            let host_part = format!("{}:{}", DEFAULT_ADDR, DEFAULT_PORT);
                            let url = format!("http://{}/", host_part);

                            let req = BrpRequest {
                                jsonrpc: String::from("2.0"),
                                method: String::from(BRP_QUERY_METHOD),
                                id: Some("1".into()),
                                params: Some(
                                    serde_json::to_value(BrpQueryParams {
                                        data: BrpQuery {
                                            components: vec![],
                                            option: vec![
                                        "bevy_core::name::Name".to_string(),
                                        "idle_rpg::game::alive::Health".to_string(),
                                        "idle_rpg::game::animation::PlayerAnimation".to_string(),
                                        "idle_rpg::game::bullets::Bullet".to_string(),
                                        "idle_rpg::game::container::Container".to_string(),
                                        "idle_rpg::game::enemy::Enemy".to_string(),
                                        "bevy_core_pipeline::bloom::settings::Bloom".to_string(),
    "bevy_core_pipeline::contrast_adaptive_sharpening::ContrastAdaptiveSharpening".to_string(),
    "bevy_core_pipeline::core_2d::camera_2d::Camera2d".to_string(),
    "bevy_core_pipeline::core_3d::camera_3d::Camera3d".to_string(),
    "bevy_hierarchy::components::children::Children".to_string(),
    "bevy_hierarchy::components::parent::Parent".to_string(),
    "bevy_mesh::morph::MeshMorphWeights".to_string(),
    "bevy_mesh::morph::MorphWeights".to_string(),
    "bevy_mesh::skinning::SkinnedMesh".to_string(),
                                        "idle_rpg::game::level::LevelEnemySpawnerTimer".to_string(),
                                        "idle_rpg::game::level::WaveController".to_string(),
                                        "idle_rpg::game::level::WaveId".to_string(),
                                        "idle_rpg::game::levelup_screen::SelectedSkill".to_string(),
                                        "idle_rpg::game::movement::MovementController".to_string(),
                                        "idle_rpg::game::movement::ScreenWrap".to_string(),
                                        "idle_rpg::game::pickable::DropPickable".to_string(),
                                        "idle_rpg::game::pickable::Pickable".to_string(),
                                        "idle_rpg::game::pickable::PickableMagnet".to_string(),
                                        // "idle_rpg::game::pickable::SpawnPickable".to_string(),
                                        "idle_rpg::game::player::Player".to_string(),
                                        "idle_rpg::game::ui::KilledEnemiesDisplay".to_string(),
                                        "idle_rpg::game::ui::PlayerHpDisplay".to_string(),
                                        "idle_rpg::game::weapon::BaseWeapon".to_string(),
                                        "idle_rpg::game::weapon::MeleeWeapon".to_string(),
                                        "idle_rpg::game::weapon::PlayerTarget".to_string(),
                                        "idle_rpg::theme::interaction::InteractionPalette".to_string(),
                                        "bevy_text::bounds::TextBounds".to_string(),
                                        "bevy_text::pipeline::TextLayoutInfo".to_string(),
                                        "bevy_text::text2d::Text2d".to_string(),
                                        "bevy_text::text::ComputedTextBlock".to_string(),
                                        "bevy_text::text::TextColor".to_string(),
                                        // "bevy_text::text::TextFont".to_string(),
                                        "bevy_text::text::TextLayout".to_string(),
                                        "bevy_text::text::TextSpan".to_string(),
                                        "bevy_transform::components::global_transform::GlobalTransform".to_string(),
                                        "bevy_transform::components::transform::Transform".to_string(),
                                        "bevy_ui::focus::FocusPolicy".to_string(),
                                        "bevy_ui::focus::Interaction".to_string(),
                                        "bevy_ui::focus::RelativeCursorPosition".to_string(),
                                        "bevy_ui::measurement::ContentSize".to_string(),
                                        "bevy_ui::ui_node::BackgroundColor".to_string(),
                                        "bevy_ui::ui_node::BorderColor".to_string(),
                                        "bevy_ui::ui_node::BorderRadius".to_string(),
                                        "bevy_ui::ui_node::CalculatedClip".to_string(),
                                        "bevy_ui::ui_node::ComputedNode".to_string(),
                                        "bevy_ui::ui_node::Node".to_string(),
                                        "bevy_ui::ui_node::Outline".to_string(),
                                        "bevy_ui::ui_node::ScrollPosition".to_string(),
                                        "bevy_ui::ui_node::TargetCamera".to_string(),
                                        "bevy_ui::ui_node::ZIndex".to_string(),
                                        "bevy_ui::widget::button::Button".to_string(),
                                        // "bevy_ui::widget::image::ImageNode".to_string(),
                                        "bevy_ui::widget::image::ImageNodeSize".to_string(),
                                        "bevy_ui::widget::label::Label".to_string(),
                                        "bevy_ui::widget::text::Text".to_string(),
                                        "bevy_ui::widget::text::TextNodeFlags".to_string(),
                                        // "bevy_sprite::sprite::Sprite".to_string(),
                                        ],
                                            has: Vec::default(),
                                        },
                                        filter: BrpQueryFilter::default(),
                                    })
                                    .expect("Unable to convert query parameters to a valid JSON value"),
                                ),
                            };

                            let request = ehttp::Request {
                                method: "GET".to_string(),
                                url,
                                body: serde_json::to_string(&req).unwrap().into_bytes(),
                                headers: Default::default(),
                            };
                            ehttp::fetch(request, move |response| {
                                *download_store.lock().unwrap() = Download::Done;
                                let Ok(response) = response else {
                                    *error_info.lock().unwrap() = Some(format!("{:#?}", &response));
                                    egui_ctx.request_repaint();
                                    return;
                                };
                                if response.ok {
                                    let json = response.text().unwrap();
                                    let result: jsonrpc_types::v2::Response =
                                        serde_json::from_str(json).unwrap();
                                    let jsonrpc_types::v2::Response::Single(result) = result else {
                                        return;
                                    };

                                    let result : jsonrpc_types::Success = match result {
                                        jsonrpc_types::Output::Success(result) => result,
                                        jsonrpc_types::Output::Failure(e) => {
                                            *error_info.lock().unwrap() = Some(format!("{:#?}", &e));
                                            return;
                                        }
                                    };
                                    let converted: BrpQueryResponse =
                                        serde_json::from_value(result.result).unwrap();
                                    *components.lock().unwrap() = converted.to_hash_map();
                                    *error_info.lock().unwrap() = None;
                                } else {
                                    *error_info.lock().unwrap() = Some(format!("{:#?}", &response));
                                }
                                egui_ctx.request_repaint(); // Wake up UI thread
                            });
                        }
                        ui.add_space(15.0);
                        ui.checkbox(&mut self.skip_empty_entities, "Hide empty entities");
                    });
                });
            ui.separator();
            ui.add_space(8.0);
            // });

            // egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let content = self.components.lock().unwrap();
                let is_empty = content.len() == 0;
                let error = self.error_info.lock().unwrap();
                if is_empty || error.is_some() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(15.0);
                        match &*error {
                            Some(e) => {
                                ui.label(
                                    RichText::new(e)
                                        .color(Color32::RED)
                                        .monospace()
                                        .line_height(Some(25.0))
                                        .size(20.0),
                                );
                            }
                            None => {
                                ui.heading("No components, try fetching first");
                            }
                        };
                        ui.add_space(15.0);
                    });
                    return;
                }
                let entities: Vec<Entity> = content
                    .iter()
                    .map(|(e, row)| {
                        if row
                            .components
                            .contains_key("bevy_hierarchy::components::parent::Parent")
                        {
                            None
                        } else {
                            Some(e.clone())
                        }
                    })
                    .flatten()
                    .collect();
                for e in entities.iter() {
                    self.draw_entity(ui, e, &content);
                }
            });
            // });
        });
    }
}

fn custom_window_frame(ctx: &egui::Context, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    use egui::{CentralPanel, UiBuilder};

    let panel_frame = egui::Frame {
        fill: ctx.style().visuals.window_fill(),
        rounding: 10.0.into(),
        stroke: ctx.style().visuals.widgets.noninteractive.fg_stroke,
        outer_margin: 0.5.into(), // so the stroke is within the bounds
        ..Default::default()
    };

    CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
        let app_rect = ui.max_rect();

        let title_bar_height = 32.0;
        let title_bar_rect = {
            let mut rect = app_rect;
            rect.max.y = rect.min.y + title_bar_height;
            rect
        };
        title_bar_ui(ui, title_bar_rect, title);

        // Add the contents:
        let content_rect = {
            let mut rect = app_rect;
            rect.min.y = title_bar_rect.max.y;
            rect
        }
        .shrink(4.0);
        let mut content_ui = ui.new_child(UiBuilder::new().max_rect(content_rect));
        add_contents(&mut content_ui);
    });
}

fn title_bar_ui(ui: &mut egui::Ui, title_bar_rect: eframe::epaint::Rect, title: &str) {
    use egui::{vec2, Align2, FontId, Id, PointerButton, Sense, UiBuilder};

    let painter = ui.painter();

    let title_bar_response = ui.interact(
        title_bar_rect,
        Id::new("title_bar"),
        Sense::click_and_drag(),
    );

    // Paint the title:
    painter.text(
        title_bar_rect.center(),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(22.0),
        egui::Color32::from_rgb(230, 102, 1),
    );

    // Paint the line under the title:
    painter.line_segment(
        [
            title_bar_rect.left_bottom() + vec2(1.0, 0.0),
            title_bar_rect.right_bottom() + vec2(-1.0, 0.0),
        ],
        ui.visuals().widgets.noninteractive.bg_stroke,
    );

    // Interact with the title bar (drag to move window):
    if title_bar_response.double_clicked() {
        let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::Maximized(!is_maximized));
    }

    if title_bar_response.drag_started_by(PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }

    ui.allocate_new_ui(
        UiBuilder::new()
            .max_rect(title_bar_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.visuals_mut().button_frame = false;
            ui.add_space(8.0);
            close_maximize_minimize(ui);
        },
    );
}

/// Show some close/maximize/minimize buttons for the native window.
fn close_maximize_minimize(ui: &mut egui::Ui) {
    use egui::{Button, RichText};

    let button_height = 12.0;

    let close_response = ui
        .add(Button::new(RichText::new("❌").size(button_height)))
        .on_hover_text("Close the window");
    if close_response.clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }

    let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    if is_maximized {
        let maximized_response = ui
            .add(Button::new(RichText::new("🗗").size(button_height)))
            .on_hover_text("Restore window");
        if maximized_response.clicked() {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::Maximized(false));
        }
    } else {
        let maximized_response = ui
            .add(Button::new(RichText::new("🗗").size(button_height)))
            .on_hover_text("Maximize window");
        if maximized_response.clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(true));
        }
    }

    let minimized_response = ui
        .add(Button::new(RichText::new("🗕").size(button_height)))
        .on_hover_text("Minimize the window");
    if minimized_response.clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}
fn setup_custom_fonts(ctx: &egui::Context) {
    // Start with the default fonts (we will be adding to them rather than replacing them).
    let mut fonts = egui::FontDefinitions::default();
    if let Ok((regular, semibold)) = get_fonts() {
        fonts
            .font_data
            .insert("regular".to_owned(), egui::FontData::from_owned(regular));
        fonts
            .font_data
            .insert("semibold".to_owned(), egui::FontData::from_owned(semibold));

        // Put my font first (highest priority) for proportional text:
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "regular".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Name("semibold".into()))
            .or_default()
            .insert(0, "semibold".to_owned());

        // Put my font as last fallback for monospace:
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("regular".to_owned());

        // Tell egui to use these fonts:
        ctx.set_fonts(fonts);
    }

    ctx.style_mut(|style| {
        for font_id in style.text_styles.values_mut() {
            font_id.size *= 1.4;
        }
    });
}

#[cfg(not(windows))]
fn get_fonts() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let font_path = std::path::Path::new("/System/Library/Fonts");

    let regular = fs::read(font_path.join("SFNSRounded.ttf"))?;
    let semibold = fs::read(font_path.join("SFCompact.ttf"))?;

    Ok((regular, semibold))
}

#[cfg(windows)]
fn get_fonts() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    use std::fs;

    let app_data = std::env::var("APPDATA")?;
    let font_path = std::path::Path::new(&app_data);

    let regular = fs::read(font_path.join("../Local/Microsoft/Windows/Fonts/aptos.ttf"))?;
    let semibold = fs::read(font_path.join("../Local/Microsoft/Windows/Fonts/aptos-semibold.ttf"))?;

    Ok((regular, semibold))
}
