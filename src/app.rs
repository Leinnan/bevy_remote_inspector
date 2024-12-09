use bevy::{
    prelude::Entity,
    remote::{
        builtin_methods::{
            BrpDestroyParams, BrpQuery, BrpQueryFilter, BrpQueryParams, BrpQueryRow,
            BRP_DESTROY_METHOD, BRP_LIST_METHOD, BRP_QUERY_METHOD,
        },
        http::{DEFAULT_ADDR, DEFAULT_PORT},
    },
    utils::HashMap,
};
use eframe::egui::{self, ViewportCommand};
use egui::{Color32, RichText};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use crate::helper;

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
    query_list: Arc<Mutex<Option<BrpQueryParams>>>,
    #[serde(skip)] // This how you opt-out of serialization of a field
    download: Arc<Mutex<Download>>,
    #[serde(skip)]
    components: Arc<Mutex<HashMap<Entity, BrpQueryRow>>>,
    skip_empty_entities: bool,
    #[serde(skip)]
    error_info: Arc<Mutex<Option<String>>>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum ActionToDo {
    #[default]
    None,
    Remove,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            download: Arc::new(Mutex::new(Download::None)),
            query_list: Arc::new(Mutex::new(None)),
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

    fn get_url(&self) -> String {
        let host_part = format!("{}:{}", DEFAULT_ADDR, DEFAULT_PORT);
        let url = format!("http://{}/", host_part);
        url
    }

    fn fetch_list(&self) {
        let download_store = self.download.clone();
        let error_info = self.error_info.clone();
        let query_param = self.query_list.clone();
        *download_store.lock().unwrap() = Download::InProgress;

        let request = helper::make_empty_request(BRP_LIST_METHOD, self.get_url());
        ehttp::fetch(request, move |response| {
            *download_store.lock().unwrap() = Download::Done;
            let Ok(response) = response else {
                *error_info.lock().unwrap() = Some(format!("{:#?}", &response));
                // egui_ctx.request_repaint();
                return;
            };
            if !response.ok {
                *error_info.lock().unwrap() = Some(format!("{:#?}", &response));
                return;
            }
            let Ok(type_list) = helper::parse(&response) else {
                *error_info.lock().unwrap() = Some(format!("{:#?}", &response));
                return;
            };

            *query_param.lock().unwrap() = Some(BrpQueryParams {
                data: BrpQuery {
                    components: vec![],
                    option: type_list,
                    has: vec![],
                },
                filter: BrpQueryFilter::default(),
            });
            *error_info.lock().unwrap() = None;
        });
    }

    fn draw_entity(
        &self,
        ui: &mut egui::Ui,
        entity: &Entity,
        components: &HashMap<Entity, BrpQueryRow>,
    ) -> ActionToDo {
        let mut action = ActionToDo::None;
        let Some(item) = components.get(entity) else {
            return action;
        };
        let is_empty = item.components.len() == 0;
        if self.skip_empty_entities && is_empty {
            return action;
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
                if ui.button("Remove entity").clicked() {
                    action = ActionToDo::Remove;
                }
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
        return action;
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
                let is_downloading =
                    matches!(&*download_store.lock().unwrap(), Download::InProgress);
                let query_param = self.query_list.clone();
                let has_query = query_param.lock().unwrap().is_some();
                if !is_downloading && !has_query {
                    self.fetch_list();
                }
                ui.add_space(8.0);
                ui.add_enabled_ui(!is_downloading && has_query, |ui| {
                    if ui.button("Fetch").clicked() {
                        let components = self.components.clone();
                        let error_info = self.error_info.clone();
                        *download_store.lock().unwrap() = Download::InProgress;
                        let egui_ctx = ctx.clone();

                        let request = helper::make_request(
                            &*query_param.lock().unwrap(),
                            BRP_QUERY_METHOD,
                            self.get_url(),
                        );
                        ehttp::fetch(request, move |response| {
                            *download_store.lock().unwrap() = Download::Done;
                            let Ok(response) = response else {
                                *error_info.lock().unwrap() = Some(format!("{:#?}", &response));
                                egui_ctx.request_repaint();
                                return;
                            };
                            if !response.ok {
                                *error_info.lock().unwrap() = Some(format!("{:#?}", &response));
                                egui_ctx.request_repaint(); // Wake up UI thread
                                return;
                            }
                            match helper::parse::<BrpQueryResponse>(&response) {
                                Ok(r) => {
                                    *components.lock().unwrap() = r.to_hash_map();
                                    *error_info.lock().unwrap() = None;
                                }
                                Err(err) => {
                                    *error_info.lock().unwrap() = Some(err);
                                }
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
                    match self.draw_entity(ui, e, &content) {
                        ActionToDo::None => {}
                        ActionToDo::Remove => {
                            let download_store = self.download.clone();
                            let request = helper::make_request(
                                &BrpDestroyParams { entity: *e },
                                BRP_DESTROY_METHOD,
                                self.get_url(),
                            );
                            ehttp::fetch(request, move |_response| {
                                *download_store.lock().unwrap() = Download::Done;
                            });
                        }
                    }
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
