extends Tree
var is_viewing_project_content: bool = false
var current_opened_project: String = ""
var current_project_folder: String = ""
var current_project_root: String = ""
var raw_project_file_tree: Array = []
@onready var collapse_btn: CheckButton = %CollapseButton
@onready var sidebar: VBoxContainer = %Sidebar
@onready var search_filter: LineEdit = %SearchFilter
@onready var select_code_btn: Button = %SelectCodeBtn
@onready var clear_focus_btn: Button = %ClearFocusBtn
@onready var toggle_collapse_btn: Button = %ToggleCollapseBtn
@onready var snapshot_btn: Button = %SnapshotBtn
@onready var explorer_btn: Button = %ExplorerButton
@onready var terminal_button: Button = %TerminalButton
@onready var refresh_btn: Button = %RefreshButton
@onready var companion_button: Button = %CompanionRun
@onready var rll_button: Button = %RLLrun
@onready var editor_select: OptionButton = %EditorSelect
var local_focus_files: Array = []
var local_focus_dirs: Array = []
var are_folders_collapsed: bool = true
const COLOR_FOLDER: Color = Color(0.95, 0.76, 0.35)
const COLOR_ROOT: Color = Color(1.0, 0.55, 0.2)
const COLOR_RUST: Color = Color(0.9, 0.45, 0.3)
const COLOR_CONFIG: Color = Color(0.35, 0.8, 0.85)
const COLOR_DOC: Color = Color(0.65, 0.85, 0.5)
const COLOR_DEFAULT_FILE: Color = Color(0.8, 0.8, 0.85)
const COLOR_FOCUSED: Color = Color(1.0, 1.0, 1.0)

func _notification(what: int) -> void:
	if what == NOTIFICATION_TRANSLATION_CHANGED:
		_setup_editor_select()
		if toggle_collapse_btn:
			toggle_collapse_btn.text = tr("UI_FOLDERS") if are_folders_collapsed else tr("UI_FOLDERS_EXPANDED")

func _ready() -> void:
	if has_node("/root/BettyBus"):
		BettyBus.message_received.connect(_on_bus_message_received)
	item_activated.connect(_on_item_activated)
	item_edited.connect(_on_item_edited)
	columns = 2
	set_column_custom_minimum_width(0, 28)
	set_column_expand(0, false)
	set_column_expand(1, true)
	hide_root = true
	if search_filter:
		search_filter.text_changed.connect(_on_search_text_changed)
	if select_code_btn:
		select_code_btn.pressed.connect(_on_select_all_code_pressed)
	if clear_focus_btn:
		clear_focus_btn.pressed.connect(_on_clear_focus_pressed)
	if toggle_collapse_btn:
		toggle_collapse_btn.pressed.connect(_on_toggle_collapse_all_pressed)
	if snapshot_btn:
		snapshot_btn.pressed.connect(_on_snapshot_pressed)
	if explorer_btn:
		explorer_btn.pressed.connect(_on_explorer_button_pressed)
	if terminal_button:
		terminal_button.pressed.connect(_on_terminal_button_pressed)
	if refresh_btn:
		refresh_btn.pressed.connect(_on_refresh_pressed)
	if companion_button:
		companion_button.pressed.connect(_on_companion_button_pressed)
	if rll_button:
		rll_button.pressed.connect(_on_rll_button_pressed)
	if collapse_btn:
		collapse_btn.toggled.connect(_on_collapse_toggled)
	_setup_editor_select()

func _setup_editor_select() -> void:
	if not editor_select: return
	var prev_idx = editor_select.selected if editor_select.item_count > 0 else 0
	editor_select.clear()
	editor_select.add_item(tr("UI_EDITOR_LITEXL"), 0)
	editor_select.add_item(tr("UI_EDITOR_SYSTEM"), 1)
	editor_select.select(prev_idx if prev_idx >= 0 else 0)

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed:
		if event.ctrl_pressed and (event.keycode == KEY_F or event.keycode == KEY_K):
			search_filter.grab_focus()
			search_filter.select_all()
			get_viewport().set_input_as_handled()
		elif event.keycode == KEY_F5 or (event.ctrl_pressed and event.keycode == KEY_R):
			_on_refresh_pressed()
			get_viewport().set_input_as_handled()
		elif event.keycode == KEY_ESCAPE:
			if search_filter.has_focus():
				search_filter.text = ""
				_on_search_text_changed("")
				search_filter.release_focus()
				get_viewport().set_input_as_handled()

func _on_search_text_changed(_new_text: String) -> void:
	if is_viewing_project_content:
		_render_project_files(current_opened_project, raw_project_file_tree)
		_restore_focus_checkboxes()

func _on_select_all_code_pressed() -> void:
	if not is_viewing_project_content: return
	var root = get_root()
	if not root: return
	_set_selection_recursive(root, true, ["rs", "gd", "toml", "yaml"])
	_on_item_edited()

func _on_clear_focus_pressed() -> void:
	if not is_viewing_project_content: return
	var root = get_root()
	if not root: return
	_set_selection_recursive(root, false)
	_on_item_edited()

func _on_snapshot_pressed() -> void:
	if not is_viewing_project_content or local_focus_files.is_empty(): return
	for file_path in local_focus_files:
		BettyBus.send_message("CreateSnapshot", {
			"relative_path": file_path,
			"reason": tr("UI_SNAPSHOT_MANUAL_REASON")
		})

func _set_selection_recursive(item: TreeItem, checked: bool, extensions: Array = []) -> void:
	if not item: return
	var meta = item.get_metadata(1)
	if meta and typeof(meta) == TYPE_DICTIONARY and meta.get("type") == "file":
		var path = meta.get("path", "")
		var matches_ext = extensions.is_empty()
		if not matches_ext:
			for ext in extensions:
				if path.ends_with("." + ext):
					matches_ext = true
					break
		if matches_ext:
			item.set_checked(0, checked)
			var base_color = meta.get("base_color", COLOR_DEFAULT_FILE)
			item.set_custom_color(1, COLOR_FOCUSED if checked else base_color)
	var child = item.get_first_child()
	while child:
		_set_selection_recursive(child, checked, extensions)
		child = child.get_next()

func _on_toggle_collapse_all_pressed() -> void:
	are_folders_collapsed = !are_folders_collapsed
	toggle_collapse_btn.text = tr("UI_FOLDERS") if are_folders_collapsed else tr("UI_FOLDERS_EXPANDED")
	var root = get_root()
	if not root: return
	_toggle_folders_recursive(root, are_folders_collapsed)

func _toggle_folders_recursive(item: TreeItem, collapse_state: bool) -> void:
	if not item: return
	var meta = item.get_metadata(1)
	if meta and typeof(meta) == TYPE_DICTIONARY and meta.get("type") == "dir":
		item.collapsed = collapse_state
	var child = item.get_first_child()
	while child:
		_toggle_folders_recursive(child, collapse_state)
		child = child.get_next()

func _get_absolute_projects_path(sub_dir: String = "") -> String:
	var candidates: Array[String] = []
	candidates.append(ProjectSettings.globalize_path("res://../projects"))
	candidates.append(ProjectSettings.globalize_path("res://projects"))
	candidates.append(OS.get_executable_path().get_base_dir().path_join("projects"))
	var chosen_base = ""
	for candidate in candidates:
		if DirAccess.dir_exists_absolute(candidate):
			chosen_base = candidate
			break
	if chosen_base.is_empty():
		chosen_base = candidates[0] if OS.is_debug_build() else candidates[2]
		DirAccess.make_dir_recursive_absolute(chosen_base)
	var target = chosen_base
	if not sub_dir.is_empty():
		var candidate_sub = chosen_base.path_join(sub_dir)
		if DirAccess.dir_exists_absolute(candidate_sub):
			target = candidate_sub
		else:
			DirAccess.make_dir_recursive_absolute(candidate_sub)
			target = candidate_sub
	if OS.get_name() == "Windows":
		target = target.replace("/", "\\")
	return target

func _on_explorer_button_pressed() -> void:
	var sub_dir = ""
	if is_viewing_project_content:
		if not current_project_folder.is_empty():
			sub_dir = current_project_folder
		elif not current_opened_project.is_empty():
			sub_dir = current_opened_project
	var abs_path = _get_absolute_projects_path(sub_dir)
	print("[Betty] Открытие проводника: ", abs_path)
	OS.shell_open(abs_path)
	BettyBus.send_message("OpenFolder", {"path": abs_path})

func _on_terminal_button_pressed() -> void:
	var sub_dir = ""
	if is_viewing_project_content:
		if not current_project_folder.is_empty():
			sub_dir = current_project_folder
		elif not current_opened_project.is_empty():
			sub_dir = current_opened_project
	var abs_path = _get_absolute_projects_path(sub_dir)
	BettyBus.send_message("OpenTerminal", {"path": abs_path})

func _on_refresh_pressed() -> void:
	_setup_editor_select()
	if is_viewing_project_content and current_opened_project != "":
		BettyBus.send_message("OpenProject", {"path": current_project_root})
	else:
		BettyBus.send_message("ScanProjects")

func _on_bus_message_received(action: String, data: Variant) -> void:
	match action:
		"UpdateAvailableBoxes":
			if not is_viewing_project_content:
				var boxes = data if typeof(data) == TYPE_ARRAY else []
				_render_project_list(boxes)
		"ProjectOpened":
			is_viewing_project_content = true
			var project_name = "Unknown"
			var files = []
			if typeof(data) == TYPE_DICTIONARY:
				project_name = data.get("name", "Unknown")
				files = data.get("file_tree", [])
				if current_project_folder.is_empty():
					if data.has("folder"):
						current_project_folder = data.get("folder")
					elif data.has("path"):
						var p = str(data.get("path")).replace("\\", "/").strip_edges()
						current_project_folder = p.get_file()
					else:
						current_project_folder = project_name
			current_opened_project = project_name
			raw_project_file_tree = files
			_render_project_files(project_name, files)
			if local_focus_files.is_empty() and local_focus_dirs.is_empty():
				_apply_default_focus_checkboxes()
			else:
				_restore_focus_checkboxes()

func _render_project_list(boxes: Array) -> void:
	clear()
	var root = create_item()
	if boxes.is_empty():
		var item = create_item(root)
		item.set_text(1, tr("UI_PROJECTS_EMPTY"))
		item.set_custom_color(1, Color(0.6, 0.6, 0.6))
		return
	var filter_text = search_filter.text.strip_edges().to_lower()
	for box in boxes:
		if not filter_text.is_empty() and not box.to_lower().contains(filter_text):
			continue
		var item = create_item(root)
		item.set_text(1, "📦  " + box)
		item.set_custom_color(1, COLOR_FOLDER)
		item.set_metadata(1, {"type": "box", "name": box})

func _render_project_files(project_name: String, file_tree_paths: Array) -> void:
	clear()
	var root = create_item()
	var back_item = create_item(root)
	back_item.set_text(1, tr("UI_BACK_TO_PROJECTS"))
	back_item.set_custom_color(1, Color(0.7, 0.7, 0.75))
	back_item.set_metadata(1, {"type": "back_button"})
	var project_root = create_item(root)
	project_root.set_text(1, "🗁  " + project_name.to_upper())
	project_root.set_custom_color(1, COLOR_ROOT)
	project_root.set_selectable(1, false)
	var filter_text = search_filter.text.strip_edges().to_lower()
	var created_dirs = {}
	for path_str in file_tree_paths:
		var clean_path = path_str.replace("\\", "/")
		if not filter_text.is_empty() and not clean_path.to_lower().contains(filter_text):
			continue
		_add_path_to_tree(clean_path, project_root, created_dirs, not filter_text.is_empty())

func _add_path_to_tree(path_str: String, root_item: TreeItem, created_dirs: Dictionary, force_expand: bool = false) -> void:
	var parts = path_str.split("/")
	var current_parent = root_item
	var current_sub_path = ""
	for i in range(parts.size()):
		var part = parts[i]
		if part == "": continue
		if current_sub_path == "": current_sub_path = part
		else: current_sub_path += "/" + part
		if i == parts.size() - 1:
			var file_item = create_item(current_parent)
			var icon_prefix = "📄 "
			var file_color = COLOR_DEFAULT_FILE
			if part.ends_with(".rs"):
				icon_prefix = "🦀 "
				file_color = COLOR_RUST
			elif part.ends_with(".yaml") or part.ends_with(".yml") or part.ends_with(".toml"):
				icon_prefix = "⚙️ "
				file_color = COLOR_CONFIG
			elif part.ends_with(".md") or part.ends_with(".txt") or part.ends_with(".json"):
				icon_prefix = "📝 "
				file_color = COLOR_DOC
			file_item.set_text(1, icon_prefix + part)
			file_item.set_custom_color(1, file_color)
			file_item.set_metadata(1, {"type": "file", "path": path_str, "base_color": file_color})
			file_item.set_cell_mode(0, TreeItem.CELL_MODE_CHECK)
			file_item.set_editable(0, true)
			file_item.set_checked(0, false)
		else:
			if not created_dirs.has(current_sub_path):
				var dir_item = create_item(current_parent)
				dir_item.set_text(1, "📁  " + part + "/")
				dir_item.set_custom_color(1, COLOR_FOLDER)
				dir_item.set_metadata(1, {"type": "dir", "path": current_sub_path})
				dir_item.set_cell_mode(0, TreeItem.CELL_MODE_CHECK)
				dir_item.set_editable(0, true)
				dir_item.set_checked(0, false)
				dir_item.collapsed = !force_expand
				created_dirs[current_sub_path] = dir_item
			current_parent = created_dirs[current_sub_path]

func _on_collapse_toggled(is_collapsed: bool) -> void:
	if not sidebar: return
	sidebar.visible = !is_collapsed

func _apply_default_focus_checkboxes() -> void:
	var default_targets = [
		"Cargo.toml", "architecture.yaml", "main.rs", "bridge.rs"
	]
	var root = get_root()
	if not root: return
	_check_and_expand_defaults_recursive(root, default_targets)
	# Вызываем актуальную сборку фокуса и сразу шлём в бэкенд:
	_on_item_edited()

func _check_and_expand_defaults_recursive(item: TreeItem, targets: Array) -> void:
	if not item: return
	var meta = item.get_metadata(1)
	if meta and typeof(meta) == TYPE_DICTIONARY and meta.get("type") == "file":
		var file_path = meta.get("path", "").replace("\\", "/")
		for target in targets:
			if file_path.ends_with(target):
				item.set_checked(0, true)
				item.set_custom_color(1, COLOR_FOCUSED)
				var parent = item.get_parent()
				while parent and parent != get_root():
					parent.collapsed = false
					var p_meta = parent.get_metadata(1)
					if p_meta and typeof(p_meta) == TYPE_DICTIONARY and p_meta.get("type") == "dir":
						parent.set_checked(0, true)
						parent.set_custom_color(1, COLOR_FOLDER)
					parent = parent.get_parent()
				break
	var child = item.get_first_child()
	while child:
		_check_and_expand_defaults_recursive(child, targets)
		child = child.get_next()

func _restore_focus_checkboxes() -> void:
	var root = get_root()
	if not root: return
	_restore_checked_recursive(root, local_focus_files, local_focus_dirs)

func _restore_checked_recursive(item: TreeItem, focus_files: Array, focus_dirs: Array) -> void:
	if not item: return
	var meta = item.get_metadata(1)
	if meta and typeof(meta) == TYPE_DICTIONARY:
		var item_type = meta.get("type", "")
		var item_path = meta.get("path", "")
		if item_type == "file" and focus_files.has(item_path):
			item.set_checked(0, true)
			item.set_custom_color(1, COLOR_FOCUSED)
		elif item_type == "dir" and focus_dirs.has(item_path):
			item.set_checked(0, true)
			item.set_custom_color(1, COLOR_FOLDER)
			
	var child = item.get_first_child()
	while child:
		_restore_checked_recursive(child, focus_files, focus_dirs)
		child = child.get_next()

func _on_item_edited() -> void:
	var edited_item = get_edited()
	if edited_item:
		var meta = edited_item.get_metadata(1)
		if meta and typeof(meta) == TYPE_DICTIONARY:
			var item_type = meta.get("type", "")
			var is_chk = edited_item.is_checked(0)
			
			if item_type == "file" and is_chk:
				# Ставим чекбоксы всем родительским папкам вверх по цепочке
				var parent = edited_item.get_parent()
				while parent and parent != get_root():
					var p_meta = parent.get_metadata(1)
					if p_meta and typeof(p_meta) == TYPE_DICTIONARY and p_meta.get("type") == "dir":
						parent.set_checked(0, true)
						parent.set_custom_color(1, COLOR_FOLDER)
					parent = parent.get_parent()
			elif item_type == "dir" and not is_chk:
				# Если сняли чекбокс с папки — снимаем со всего содержимого внутри
				_uncheck_children_recursive(edited_item)

	var code_files: Array = []
	var outline_dirs: Array = []
	var root = get_root()
	if not root: return
	_collect_checked_elements(root, code_files, outline_dirs)
	
	local_focus_files = code_files
	local_focus_dirs = outline_dirs
	BettyBus.send_message("UpdateFocus", {
		"code_files": code_files,
		"outline_dirs": outline_dirs
	})

func _uncheck_children_recursive(item: TreeItem) -> void:
	var child = item.get_first_child()
	while child:
		child.set_checked(0, false)
		var meta = child.get_metadata(1)
		if meta and typeof(meta) == TYPE_DICTIONARY:
			if meta.get("type") == "file":
				child.set_custom_color(1, meta.get("base_color", COLOR_DEFAULT_FILE))
		_uncheck_children_recursive(child)
		child = child.get_next()

func _collect_checked_elements(item: TreeItem, code_files: Array, outline_dirs: Array) -> void:
	if not item: return
	var meta = item.get_metadata(1)
	if meta and typeof(meta) == TYPE_DICTIONARY:
		var item_type = meta.get("type")
		var is_chk = item.is_checked(0)
		if item_type == "file":
			var base_color = meta.get("base_color", COLOR_DEFAULT_FILE)
			if is_chk:
				item.set_custom_color(1, COLOR_FOCUSED)
				var file_path = meta.get("path", "")
				if file_path != "":
					code_files.append(file_path)
			else:
				item.set_custom_color(1, base_color)
		elif item_type == "dir":
			if is_chk:
				var dir_path = meta.get("path", "")
				if dir_path != "":
					outline_dirs.append(dir_path)
	var child = item.get_first_child()
	while child:
		_collect_checked_elements(child, code_files, outline_dirs)
		child = child.get_next()

func _on_companion_button_pressed() -> void:
	BettyBus.send_message("OpenCompanion", {"url": "https://gemini.google.com"})

func _on_rll_button_pressed() -> void:
	if current_project_root != "":
		BettyBus.send_message("OpenRll", { "project_path": current_project_root })

func _on_item_activated() -> void:
	var selected_item = get_selected()
	if not selected_item: return
	var meta = selected_item.get_metadata(1)
	if not meta or typeof(meta) != TYPE_DICTIONARY: return
	match meta.get("type"):
		"box":
			var box_name = meta.get("name")
			current_project_folder = box_name
			current_opened_project = box_name
			current_project_root = "./projects/" + box_name
			BettyBus.send_message("OpenProject", {"path": current_project_root})
		"back_button":
			is_viewing_project_content = false
			current_project_folder = ""
			current_project_root = ""
			current_opened_project = ""
			raw_project_file_tree.clear()
			local_focus_files.clear()
			local_focus_dirs.clear()
			BettyBus.send_message("CloseProject")
			BettyBus.send_message("ScanProjects")
		"file":
			var relative_path = meta.get("path", "")
			var full_project_path = (current_project_root + "/" + relative_path).replace("//", "/")
			var editor_mode = "lite_xl"
			if editor_select and editor_select.selected == 1:
				editor_mode = "system"
			if has_node("/root/BettyBus"):
				BettyBus.send_message("OpenFile", {"path": full_project_path, "editor": editor_mode})
