//! AppKit front end.
//!
//! This mirrors the Win32 front end. The window and its controls are created and ordered onto
//! the screen before any database work starts, a worker thread loads the history, and the result
//! is handed back to the main thread.

use crate::history::{self, Entry};
use crate::theme::{self, Color, Theme};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAlertStyle, NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSAutoresizingMaskOptions,
    NSBackingStoreType, NSBezierPath, NSBorderType, NSColor, NSControl,
    NSControlTextEditingDelegate, NSEventModifierFlags, NSFocusRingType, NSFont, NSLineBreakMode,
    NSMenu, NSMenuItem, NSPopUpButton, NSResponder, NSScrollView, NSTableCellView, NSTableColumn,
    NSTableColumnResizingOptions, NSTableRowView, NSTableView, NSTableViewColumnAutoresizingStyle,
    NSTableViewDataSource, NSTableViewDelegate, NSTableViewStyle, NSTextField,
    NSTextFieldBezelStyle, NSTextFieldDelegate, NSTextView, NSView, NSWindow, NSWindowStyleMask,
    NSWorkspace,
};
use objc2_foundation::{
    NSArray, NSIndexSet, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect,
    NSSize, NSString, NSUInteger, ns_string,
};
use std::cell::{OnceCell, RefCell};
use std::ffi::c_void;
use std::process::{Command, Stdio};

const WINDOW_WIDTH: f64 = 620.0;
const WINDOW_HEIGHT: f64 = 500.0;
const MARGIN: f64 = 12.0;
const GAP: f64 = 8.0;
const REMOTE_WIDTH: f64 = 180.0;
const ROW_HEIGHT: f64 = 28.0;
const ROW_PADDING: f64 = 10.0;
const PAGE_ROWS: NSInteger = 8;
const VSCODE_BUNDLE_ID: &str = "com.microsoft.VSCode";

type LoadResult = Result<Vec<Entry>, String>;

thread_local! {
    /// The controller, for work handed back to the main thread.
    static CONTROLLER: OnceCell<Retained<Controller>> = const { OnceCell::new() };
}

// libdispatch is part of libSystem. `dispatch_get_main_queue()` is a C macro for the address of
// `_dispatch_main_q`.
#[repr(C)]
struct DispatchQueue {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    static _dispatch_main_q: DispatchQueue;
    fn dispatch_async_f(
        queue: *const DispatchQueue,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

struct AppState {
    entries: Vec<Entry>,
    shown: Vec<usize>,
    remote_keys: Vec<Option<String>>,
    loaded: bool,
    load_error: Option<String>,
    /// Shown as a single unselectable row instead of the entries.
    message: Option<String>,
}

struct ControllerIvars {
    theme: Theme,
    window: Retained<NSWindow>,
    filter: Retained<NSTextField>,
    remote_filter: Retained<NSPopUpButton>,
    list: Retained<NSTableView>,
    state: RefCell<AppState>,
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `Controller` does not implement
    // `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "VSRecentController"]
    #[ivars = ControllerIvars]
    struct Controller;

    impl Controller {
        #[unsafe(method(remoteFilterChanged:))]
        fn remote_filter_changed(&self, _sender: Option<&AnyObject>) {
            self.apply_filter();
        }

        #[unsafe(method(rowDoubleClicked:))]
        fn row_double_clicked(&self, _sender: Option<&AnyObject>) {
            if self.ivars().list.clickedRow() >= 0 {
                self.launch_selected(false);
            }
        }

        #[unsafe(method(openInNewWindow:))]
        fn open_in_new_window(&self, _sender: Option<&AnyObject>) {
            self.launch_selected(true);
        }
    }

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSApplicationDelegate for Controller {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            // Launch Services activates a bundled app by itself. This covers running the bare
            // executable from a terminal.
            #[allow(deprecated)]
            NSApplication::sharedApplication(self.mtm()).activateIgnoringOtherApps(true);
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate_after_last_window_closed(&self, _sender: &NSApplication) -> bool {
            true
        }

        #[unsafe(method(applicationSupportsSecureRestorableState:))]
        fn supports_secure_restorable_state(&self, _sender: &NSApplication) -> bool {
            true
        }
    }

    unsafe impl NSControlTextEditingDelegate for Controller {
        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, _notification: &NSNotification) {
            self.apply_filter();
        }

        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn control_do_command(
            &self,
            _control: &NSControl,
            _text_view: &NSTextView,
            command: Sel,
        ) -> bool {
            self.handle_picker_command(command)
        }
    }

    unsafe impl NSTextFieldDelegate for Controller {}

    unsafe impl NSTableViewDataSource for Controller {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _list: &NSTableView) -> NSInteger {
            let state = self.ivars().state.borrow();
            if state.message.is_some() {
                1
            } else {
                state.shown.len() as NSInteger
            }
        }
    }

    unsafe impl NSTableViewDelegate for Controller {
        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn view_for_row(
            &self,
            list: &NSTableView,
            _column: Option<&NSTableColumn>,
            row: NSInteger,
        ) -> Option<Retained<NSView>> {
            self.row_cell(row, list.selectedRow() == row)
        }

        #[unsafe(method_id(tableView:rowViewForRow:))]
        fn row_view_for_row(
            &self,
            _list: &NSTableView,
            row: NSInteger,
        ) -> Option<Retained<NSTableRowView>> {
            self.row_view(row)
        }

        #[unsafe(method(tableView:shouldSelectRow:))]
        fn should_select_row(&self, _list: &NSTableView, _row: NSInteger) -> bool {
            self.ivars().state.borrow().message.is_none()
        }

        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_did_change(&self, _notification: &NSNotification) {
            self.refresh_row_text_colors();
        }
    }
);

impl Controller {
    /// Creates the window and its controls. Nothing here touches the history database.
    fn new(mtm: MainThreadMarker, theme: Theme) -> Retained<Self> {
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        // SAFETY: The window is owned through `Retained`, so AppKit must not also release it when
        // it closes.
        let window = unsafe {
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect(0.0, 0.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                style,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window
        };
        window.setTitle(ns_string!("VS Recent"));
        window.setContentMinSize(NSSize::new(380.0, 220.0));
        window.setRestorable(false);
        window.center();

        let content = window
            .contentView()
            .expect("a titled window has a content view");
        let bounds = content.bounds();
        let (width, height) = (bounds.size.width, bounds.size.height);

        let filter = NSTextField::textFieldWithString(ns_string!(""), mtm);
        filter.setPlaceholderString(Some(ns_string!("Type to filter")));
        filter.setBezelStyle(NSTextFieldBezelStyle::RoundedBezel);

        let remote_filter =
            NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
        remote_filter.addItemWithTitle(ns_string!("All remotes"));

        // The controls report their natural heights, which differ between macOS releases.
        filter.sizeToFit();
        remote_filter.sizeToFit();
        let filter_height = filter.frame().size.height;
        let remote_height = remote_filter.frame().size.height;
        let controls_height = filter_height.max(remote_height);
        let controls_bottom = height - MARGIN - controls_height;
        filter.setFrame(rect(
            MARGIN,
            controls_bottom + ((controls_height - filter_height) / 2.0).floor(),
            width - MARGIN * 2.0 - GAP - REMOTE_WIDTH,
            filter_height,
        ));
        filter.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMinYMargin,
        );
        remote_filter.setFrame(rect(
            width - MARGIN - REMOTE_WIDTH,
            controls_bottom + ((controls_height - remote_height) / 2.0).floor(),
            REMOTE_WIDTH,
            remote_height,
        ));
        remote_filter.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMinYMargin,
        );

        let footer = NSTextField::labelWithString(
            ns_string!(
                "\u{2191}\u{2193} Select  |  \u{21a9} Open  |  \u{2318}\u{21a9} New Window  |  \u{21e7}\u{21a9} Keep Open  |  esc Close"
            ),
            mtm,
        );
        footer.setFont(Some(&NSFont::systemFontOfSize(
            NSFont::smallSystemFontSize(),
        )));
        footer.setTextColor(Some(&NSColor::secondaryLabelColor()));
        footer.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        footer.sizeToFit();
        let footer_height = footer.frame().size.height;
        footer.setFrame(rect(
            MARGIN,
            MARGIN - 2.0,
            width - MARGIN * 2.0,
            footer_height,
        ));
        footer.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        let list_bottom = MARGIN + footer_height + GAP;
        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            rect(
                MARGIN,
                list_bottom,
                width - MARGIN * 2.0,
                controls_bottom - GAP - list_bottom,
            ),
        );
        scroll.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        scroll.setBorderType(NSBorderType::BezelBorder);
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);

        let list_size = scroll.contentSize();
        let list = NSTableView::initWithFrame(
            NSTableView::alloc(mtm),
            rect(0.0, 0.0, list_size.width, list_size.height),
        );
        let column =
            NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), ns_string!("folder"));
        column.setWidth(list_size.width);
        column.setResizingMask(NSTableColumnResizingOptions::AutoresizingMask);
        column.setEditable(false);
        list.addTableColumn(&column);
        list.setColumnAutoresizingStyle(
            NSTableViewColumnAutoresizingStyle::UniformColumnAutoresizingStyle,
        );
        list.setHeaderView(None);
        list.setStyle(NSTableViewStyle::Plain);
        list.setRowHeight(ROW_HEIGHT);
        list.setIntercellSpacing(NSSize::new(0.0, 0.0));
        list.setBackgroundColor(&ns_color(theme.control));
        list.setAllowsEmptySelection(false);
        list.setAllowsColumnReordering(false);
        list.setAllowsColumnResizing(false);
        // Keep typing in the filter while rows are clicked.
        list.setRefusesFirstResponder(true);
        list.setFocusRingType(NSFocusRingType::None);
        scroll.setDocumentView(Some(&list));

        content.addSubview(&filter);
        content.addSubview(&remote_filter);
        content.addSubview(&scroll);
        content.addSubview(&footer);

        let this = Self::alloc(mtm).set_ivars(ControllerIvars {
            theme,
            window,
            filter,
            remote_filter,
            list,
            state: RefCell::new(AppState {
                entries: Vec::new(),
                shown: Vec::new(),
                remote_keys: vec![None],
                loaded: false,
                load_error: None,
                message: Some(history::LOADING_MESSAGE.to_string()),
            }),
        });
        // SAFETY: `NSObject`'s `init` takes no arguments and returns the initialized receiver.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };

        let ivars = this.ivars();
        let target: &AnyObject = this.as_ref();
        // SAFETY: The controls keep unretained references to the controller, which lives in
        // `CONTROLLER` until the process exits. The selectors are defined on `Controller` with
        // the signature AppKit expects for actions.
        unsafe {
            ivars
                .filter
                .setDelegate(Some(ProtocolObject::from_ref(&*this)));
            ivars
                .list
                .setDataSource(Some(ProtocolObject::from_ref(&*this)));
            ivars
                .list
                .setDelegate(Some(ProtocolObject::from_ref(&*this)));
            ivars.list.setTarget(Some(target));
            ivars.list.setDoubleAction(Some(sel!(rowDoubleClicked:)));
            ivars.remote_filter.setTarget(Some(target));
            ivars
                .remote_filter
                .setAction(Some(sel!(remoteFilterChanged:)));
        }
        ivars.window.setInitialFirstResponder(Some(&ivars.filter));
        ivars.window.makeFirstResponder(Some(&ivars.filter));
        this
    }

    fn show(&self) {
        self.ivars().window.makeKeyAndOrderFront(None);
    }

    /// The label for a row: an entry, or the status message when there are no entries to show.
    fn row_cell(&self, row: NSInteger, selected: bool) -> Option<Retained<NSView>> {
        let ivars = self.ivars();
        let state = ivars.state.borrow();
        let (text, color) = match &state.message {
            Some(message) => (message.clone(), ns_color(ivars.theme.muted)),
            None => {
                let color = if selected {
                    NSColor::whiteColor()
                } else {
                    ns_color(ivars.theme.text)
                };
                (history::row_text(shown_entry(&state, row)?), color)
            }
        };
        Some(row_cell(&text, &color, self.mtm()))
    }

    /// The background for a row, colored by the entry's remote kind.
    fn row_view(&self, row: NSInteger) -> Option<Retained<NSTableRowView>> {
        let ivars = self.ivars();
        let state = ivars.state.borrow();
        let (normal, selected) = match &state.message {
            Some(_) => (ivars.theme.control, ivars.theme.control),
            None => theme::row_colors(&shown_entry(&state, row)?.remote, ivars.theme.dark),
        };
        let row_view = RowView::new(self.mtm(), ns_color(normal), ns_color(selected));
        Some(Retained::into_super(row_view))
    }

    fn entries_loaded(&self, loaded: LoadResult) {
        {
            let mut state = self.ivars().state.borrow_mut();
            state.loaded = true;
            match loaded {
                Ok(entries) => {
                    state.entries = entries;
                    state.load_error = None;
                }
                Err(error) => {
                    state.load_error = Some(format!("Unable to load history: {error}"));
                }
            }
        }
        self.populate_remote_filter();
        self.apply_filter();
    }

    fn populate_remote_filter(&self) {
        let ivars = self.ivars();
        let (all, groups) = {
            let state = ivars.state.borrow();
            (
                history::all_remotes_title(state.entries.len()),
                history::remote_groups(&state.entries),
            )
        };
        ivars.remote_filter.removeAllItems();
        ivars
            .remote_filter
            .addItemWithTitle(&NSString::from_str(&all));
        let mut remote_keys = vec![None];
        for (key, title) in groups {
            ivars
                .remote_filter
                .addItemWithTitle(&NSString::from_str(&title));
            remote_keys.push(Some(key));
        }
        ivars.remote_filter.selectItemAtIndex(0);
        ivars.state.borrow_mut().remote_keys = remote_keys;
    }

    fn apply_filter(&self) {
        let ivars = self.ivars();
        let query = ivars.filter.stringValue().to_string();
        let remote_index = ivars.remote_filter.indexOfSelectedItem();
        let has_rows = {
            let mut state = ivars.state.borrow_mut();
            let state = &mut *state;
            let remote = usize::try_from(remote_index)
                .ok()
                .and_then(|index| state.remote_keys.get(index))
                .and_then(|key| key.as_deref());
            state.shown = history::matching_entries(&state.entries, &query, remote);
            state.message = if state.loaded {
                history::status_message(
                    state.load_error.as_deref(),
                    state.entries.len(),
                    state.shown.len(),
                )
            } else {
                Some(history::LOADING_MESSAGE.to_string())
            };
            state.message.is_none()
        };
        // The borrow is released first because reloading calls back into the data source.
        ivars.list.reloadData();
        if has_rows {
            self.select_row(0);
        }
    }

    fn select_row(&self, row: NSInteger) {
        let list = &self.ivars().list;
        // AppKit raises an exception for a row it does not have, so its count is the authority.
        if row < 0 || row >= list.numberOfRows() {
            return;
        }
        let indexes = NSIndexSet::indexSetWithIndex(row as NSUInteger);
        list.selectRowIndexes_byExtendingSelection(&indexes, false);
        list.scrollRowToVisible(row);
        self.refresh_row_text_colors();
    }

    fn move_selection(&self, delta: NSInteger) {
        let count = self.selectable_rows();
        if count == 0 {
            return;
        }
        let current = self.ivars().list.selectedRow().max(0);
        self.select_row((current + delta).clamp(0, count - 1));
    }

    fn selectable_rows(&self) -> NSInteger {
        let state = self.ivars().state.borrow();
        if state.message.is_some() {
            0
        } else {
            state.shown.len() as NSInteger
        }
    }

    /// Selected rows use white text on the stronger row color.
    fn refresh_row_text_colors(&self) {
        let ivars = self.ivars();
        let count = self.selectable_rows().min(ivars.list.numberOfRows());
        let selected = ivars.list.selectedRow();
        let normal = ns_color(ivars.theme.text);
        let highlighted = NSColor::whiteColor();
        for row in 0..count {
            // Only rows that are on screen, or prepared for scrolling, have views.
            let Some(view) = ivars.list.viewAtColumn_row_makeIfNecessary(0, row, false) else {
                continue;
            };
            let Ok(cell) = view.downcast::<NSTableCellView>() else {
                continue;
            };
            // SAFETY: `row_cell` sets the text field to a label that the cell view retains.
            if let Some(label) = unsafe { cell.textField() } {
                label.setTextColor(Some(if row == selected {
                    &highlighted
                } else {
                    &normal
                }));
            }
        }
    }

    /// Handles the editing commands that the filter's field editor would otherwise apply to the
    /// text, so the input system still sees every key first.
    fn handle_picker_command(&self, command: Sel) -> bool {
        if command == sel!(insertNewline:)
            || command == sel!(insertLineBreak:)
            || command == sel!(insertNewlineIgnoringFieldEditor:)
        {
            let force_new = current_modifiers(self.mtm()).contains(NSEventModifierFlags::Command);
            self.launch_selected(force_new);
        } else if command == sel!(cancelOperation:) {
            self.ivars().window.close();
        } else if command == sel!(moveDown:) {
            self.move_selection(1);
        } else if command == sel!(moveUp:) {
            self.move_selection(-1);
        } else if command == sel!(pageDown:) || command == sel!(scrollPageDown:) {
            self.move_selection(PAGE_ROWS);
        } else if command == sel!(pageUp:) || command == sel!(scrollPageUp:) {
            self.move_selection(-PAGE_ROWS);
        } else if command == sel!(moveToBeginningOfDocument:)
            || command == sel!(scrollToBeginningOfDocument:)
        {
            self.move_selection(NSInteger::MIN / 2);
        } else if command == sel!(moveToEndOfDocument:) || command == sel!(scrollToEndOfDocument:) {
            self.move_selection(NSInteger::MAX / 2);
        } else {
            return false;
        }
        true
    }

    fn launch_selected(&self, force_new: bool) {
        let ivars = self.ivars();
        let uri = {
            let state = ivars.state.borrow();
            if state.message.is_some() {
                return;
            }
            match shown_entry(&state, ivars.list.selectedRow()) {
                Some(entry) => entry.uri.clone(),
                None => return,
            }
        };
        let keep_open = current_modifiers(self.mtm()).contains(NSEventModifierFlags::Shift);
        match open_folder(&uri, force_new) {
            Ok(()) if !keep_open => ivars.window.close(),
            Ok(()) => {}
            Err(error) => show_error(self.mtm(), &error),
        }
    }
}

struct RowColors {
    normal: Retained<NSColor>,
    selected: Retained<NSColor>,
}

define_class!(
    // SAFETY: `NSTableRowView` is designed to be subclassed to customize row drawing, and
    // `RowView` does not implement `Drop`.
    #[unsafe(super(NSTableRowView, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "VSRecentRowView"]
    #[ivars = RowColors]
    struct RowView;

    impl RowView {
        #[unsafe(method(drawBackgroundInRect:))]
        fn draw_background(&self, _dirty: NSRect) {
            let colors = self.ivars();
            if self.isSelected() {
                colors.selected.setFill();
            } else {
                colors.normal.setFill();
            }
            NSBezierPath::fillRect(self.bounds());
        }

        // The selection keeps its remote color whether or not the window is active.
        #[unsafe(method(drawSelectionInRect:))]
        fn draw_selection(&self, _dirty: NSRect) {
            self.ivars().selected.setFill();
            NSBezierPath::fillRect(self.bounds());
        }
    }
);

impl RowView {
    fn new(
        mtm: MainThreadMarker,
        normal: Retained<NSColor>,
        selected: Retained<NSColor>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RowColors { normal, selected });
        // SAFETY: `initWithFrame:` is the designated initializer of `NSView` subclasses.
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
    }
}

pub fn main() {
    let mtm = MainThreadMarker::new().expect("the process starts on the main thread");
    let arguments: Vec<String> = std::env::args().collect();
    let app = NSApplication::sharedApplication(mtm);
    // A bundled app is already a regular app. This gives the bare executable a Dock icon and
    // menu bar too.
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let theme = match theme::requested_theme(&arguments) {
        Some(theme) => {
            // SAFETY: The appearance names are constant strings exported by AppKit.
            let name = unsafe {
                if theme.dark {
                    NSAppearanceNameDarkAqua
                } else {
                    NSAppearanceNameAqua
                }
            };
            app.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
            theme
        }
        None => system_theme(&app),
    };

    let controller = Controller::new(mtm, theme);
    app.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
    install_menu(&app, &controller, mtm);
    controller.show();
    CONTROLLER.with(|cell| {
        let _ = cell.set(controller);
    });

    let demo = arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--demo"));
    std::thread::spawn(move || {
        let loaded = if demo {
            Ok(history::demo_entries())
        } else {
            history::load_entries()
        };
        let payload = Box::into_raw(Box::new(loaded));
        // SAFETY: The main queue outlives the process, and `deliver_entries` takes ownership of
        // the payload exactly once.
        unsafe { dispatch_async_f(&raw const _dispatch_main_q, payload.cast(), deliver_entries) };
    });

    app.run();
}

/// Runs on the main thread once the worker has loaded the history.
extern "C" fn deliver_entries(payload: *mut c_void) {
    // SAFETY: `payload` is the box that the worker thread leaked for this call.
    let loaded = unsafe { Box::from_raw(payload.cast::<LoadResult>()) };
    CONTROLLER.with(|cell| {
        if let Some(controller) = cell.get() {
            controller.entries_loaded(*loaded);
        }
    });
}

fn system_theme(app: &NSApplication) -> Theme {
    // SAFETY: The appearance names are constant strings exported by AppKit.
    let (aqua, dark_aqua) = unsafe { (NSAppearanceNameAqua, NSAppearanceNameDarkAqua) };
    let best = app
        .effectiveAppearance()
        .bestMatchFromAppearancesWithNames(&NSArray::from_slice(&[aqua, dark_aqua]));
    if best.is_some_and(|name| &*name == dark_aqua) {
        Theme::dark()
    } else {
        Theme::light()
    }
}

/// Text editing shortcuts are menu key equivalents on macOS, so even a window this small needs a
/// menu bar for Cut, Copy, Paste, and Select All to work in the filter.
fn install_menu(app: &NSApplication, controller: &Controller, mtm: MainThreadMarker) {
    let main_menu = NSMenu::new(mtm);
    let add_menu = |title: &NSString| {
        let item = NSMenuItem::new(mtm);
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), title);
        item.setSubmenu(Some(&menu));
        main_menu.addItem(&item);
        menu
    };
    let add_item = |menu: &NSMenu, title: &NSString, action: Sel, key: &NSString| {
        // SAFETY: Every action is a standard AppKit action or one defined on `Controller`, and
        // all of them take the sender as their only argument.
        unsafe { menu.addItemWithTitle_action_keyEquivalent(title, Some(action), key) }
    };

    let app_menu = add_menu(ns_string!("VS Recent"));
    add_item(
        &app_menu,
        ns_string!("Hide VS Recent"),
        sel!(hide:),
        ns_string!("h"),
    );
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    add_item(
        &app_menu,
        ns_string!("Quit VS Recent"),
        sel!(terminate:),
        ns_string!("q"),
    );

    let file_menu = add_menu(ns_string!("File"));
    let open_in_new_window = add_item(
        &file_menu,
        ns_string!("Open in New Window"),
        sel!(openInNewWindow:),
        ns_string!("\r"),
    );
    // SAFETY: The controller lives in `CONTROLLER` until the process exits.
    unsafe { open_in_new_window.setTarget(Some(controller.as_ref())) };
    add_item(
        &file_menu,
        ns_string!("Close"),
        sel!(performClose:),
        ns_string!("w"),
    );

    let edit_menu = add_menu(ns_string!("Edit"));
    add_item(&edit_menu, ns_string!("Undo"), sel!(undo:), ns_string!("z"));
    add_item(&edit_menu, ns_string!("Redo"), sel!(redo:), ns_string!("Z"));
    edit_menu.addItem(&NSMenuItem::separatorItem(mtm));
    add_item(&edit_menu, ns_string!("Cut"), sel!(cut:), ns_string!("x"));
    add_item(&edit_menu, ns_string!("Copy"), sel!(copy:), ns_string!("c"));
    add_item(
        &edit_menu,
        ns_string!("Paste"),
        sel!(paste:),
        ns_string!("v"),
    );
    add_item(
        &edit_menu,
        ns_string!("Select All"),
        sel!(selectAll:),
        ns_string!("a"),
    );

    app.setMainMenu(Some(&main_menu));
}

/// Opens the folder the way VS Code's own `code` command does on macOS: a second instance hands
/// the arguments to the running one, which then takes focus. Launching through Launch Services,
/// rather than through `code`, also lets a cold start resolve the user's shell environment.
fn open_folder(uri: &str, force_new: bool) -> Result<(), String> {
    let application = NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(VSCODE_BUNDLE_ID))
        .and_then(|url| url.path())
        .ok_or_else(|| "could not find Visual Studio Code".to_string())?
        .to_string();
    let mut command = Command::new("/usr/bin/open");
    command.args(["-n", "-g", "-a", &application, "--args"]);
    if force_new {
        command.arg("--new-window");
    }
    let mut child = command
        .args(["--folder-uri", uri])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start VS Code ({error})"))?;
    // Reap `open` in case the picker stays visible.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn show_error(mtm: MainThreadMarker, message: &str) {
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Critical);
    alert.setMessageText(ns_string!("VS Recent"));
    alert.setInformativeText(&NSString::from_str(message));
    alert.runModal();
}

/// The modifier keys held for the event that is being handled.
fn current_modifiers(mtm: MainThreadMarker) -> NSEventModifierFlags {
    NSApplication::sharedApplication(mtm)
        .currentEvent()
        .map(|event| event.modifierFlags())
        .unwrap_or(NSEventModifierFlags::empty())
}

fn shown_entry(state: &AppState, row: NSInteger) -> Option<&Entry> {
    let index = *state.shown.get(usize::try_from(row).ok()?)?;
    state.entries.get(index)
}

/// A cell view holding one vertically centered, tail-truncated label.
fn row_cell(text: &str, color: &NSColor, mtm: MainThreadMarker) -> Retained<NSView> {
    let width = 200.0;
    let cell = NSTableCellView::initWithFrame(
        NSTableCellView::alloc(mtm),
        rect(0.0, 0.0, width, ROW_HEIGHT),
    );
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    label.setTextColor(Some(color));
    label.sizeToFit();
    let height = label.frame().size.height;
    label.setFrame(rect(
        ROW_PADDING,
        ((ROW_HEIGHT - height) / 2.0).floor(),
        width - ROW_PADDING * 2.0,
        height,
    ));
    label.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable
            | NSAutoresizingMaskOptions::ViewMinYMargin
            | NSAutoresizingMaskOptions::ViewMaxYMargin,
    );
    cell.addSubview(&label);
    // SAFETY: The cell view retains the label as its subview.
    unsafe { cell.setTextField(Some(&label)) };
    Retained::into_super(cell)
}

fn ns_color(color: Color) -> Retained<NSColor> {
    let channel = |shift: u32| f64::from((color >> shift) as u8) / 255.0;
    NSColor::colorWithSRGBRed_green_blue_alpha(channel(0), channel(8), channel(16), 1.0)
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}
