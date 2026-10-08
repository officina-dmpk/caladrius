//! The File menu, its keyboard shortcuts and the question "save the changes?" shown before work
//! would be lost. Each entry is an [`Action`]; the logic is in [`crate::projectfile`].

use egui::{Context, Key, KeyboardShortcut, Modifiers, RichText, Ui};

use crate::app::Action;
use crate::projectfile::Guarded;
use crate::theme::Tokens;

pub const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
pub const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
pub const SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);

/// The shortcuts, read at the start of the frame. The most specific one is tested first, so
/// that Ctrl+Shift+S is not taken for Ctrl+S.
pub fn shortcuts(ctx: &Context, actions: &mut Vec<Action>) {
    ctx.input_mut(|i| {
        if i.consume_shortcut(&SAVE_AS) {
            actions.push(Action::SaveProjectAs);
        } else if i.consume_shortcut(&SAVE) {
            actions.push(Action::SaveProject);
        } else if i.consume_shortcut(&OPEN) {
            actions.push(Action::OpenProject);
        } else if i.consume_shortcut(&NEW) {
            actions.push(Action::NewProject);
        }
    });
}

/// The File menu button of the top bar.
pub fn file_menu(ui: &mut Ui, actions: &mut Vec<Action>) {
    egui::containers::menu::MenuButton::new("File").ui(ui, |ui| {
        let ctx = ui.ctx().clone();
        let mut entry =
            |ui: &mut Ui, text: &str, shortcut: Option<&KeyboardShortcut>, a: Action| {
                let mut button = egui::Button::new(text);
                if let Some(s) = shortcut {
                    button = button.shortcut_text(ctx.format_shortcut(s));
                }
                if ui.add(button).clicked() {
                    actions.push(a);
                }
            };
        entry(ui, "New project", Some(&NEW), Action::NewProject);
        entry(ui, "Open project…", Some(&OPEN), Action::OpenProject);
        entry(ui, "Open CSV…", None, Action::OpenCsv);
        ui.separator();
        entry(ui, "Save", Some(&SAVE), Action::SaveProject);
        entry(ui, "Save as…", Some(&SAVE_AS), Action::SaveProjectAs);
        ui.separator();
        entry(ui, "Quit", None, Action::Quit);
    });
}

/// The sentence of the question: what is at stake and what the person asked for.
pub fn guard_sentence(project: &str, what: &Guarded) -> String {
    let loss = match what {
        Guarded::NewProject => "Starting a new project would lose them.",
        Guarded::OpenProject | Guarded::OpenBytes { .. } => {
            "Opening another project would lose them."
        }
        Guarded::Close => "Closing Caladrius would lose them.",
    };
    format!("The project \"{project}\" has changes that are not saved. {loss}")
}

/// The question with its three answers, over the screen.
pub fn guard_dialog(
    ctx: &Context,
    tokens: &Tokens,
    project: &str,
    what: &Guarded,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    let modal = egui::Modal::new(egui::Id::new("unsaved-changes")).show(ctx, |ui| {
        ui.set_max_width(tokens.size.dialog_width);
        ui.label(
            RichText::new("Save the changes?")
                .size(tokens.font.heading)
                .strong(),
        );
        ui.add_space(tokens.spacing.small);
        ui.label(guard_sentence(project, what));
        ui.add_space(tokens.spacing.medium);
        ui.horizontal(|ui| {
            if ui.add(tokens.primary_button("Save")).clicked() {
                actions.push(Action::GuardSave);
            }
            if ui.button("Don't save").clicked() {
                actions.push(Action::GuardDiscard);
            }
            if ui.button("Cancel").clicked() {
                actions.push(Action::GuardCancel);
            }
        });
        ui.add_space(tokens.spacing.small);
        ui.label(
            RichText::new("Escape cancels.")
                .small()
                .color(c.text_muted.color()),
        );
    });
    if modal.should_close() {
        actions.push(Action::GuardCancel);
    }
}
