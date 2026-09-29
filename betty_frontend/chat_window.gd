extends Control
@onready var scroll_container = $ScrollContainer
@onready var chat_history = $ScrollContainer/VBoxContainer
@onready var input_field = $InputBar/TextEdit
@onready var send_button = $InputBar/Button
@onready var model_selector = %OptionButton
@onready var status_bar = %StatusBar
@onready var settings_button = %SettingsButton
var _current_stream_panel: PanelContainer = null
var _current_stream_label: RichTextLabel = null
var _stream_text_buffer: String = ""
var _settings_dialog_scene = preload("res://settings_dialog.tscn")
var _settings_dialog_instance: Window = null
var _cached_settings: Dictionary = {}

func _ready() -> void:
	if has_node("/root/BettyBus"):
		BettyBus.message_received.connect(_on_bus_message_received)
		BettyBus.connection_established.connect(_on_bus_connected)
	if settings_button:
		settings_button.pressed.connect(_on_settings_button_pressed)
	send_button.pressed.connect(_on_send_pressed)
	input_field.gui_input.connect(_on_input_field_gui_input)
	input_field.text_changed.connect(_on_input_field_text_changed)

func _on_bus_connected() -> void:
	BettyBus.send_message("GetSettings")

func _on_settings_button_pressed() -> void:
	if not _settings_dialog_instance:
		_settings_dialog_instance = _settings_dialog_scene.instantiate()
		_settings_dialog_instance.settings_saved.connect(func():
			BettyBus.send_message("GetSettings")
		)
		add_child(_settings_dialog_instance)
	if not _cached_settings.is_empty():
		_settings_dialog_instance.populate_from_settings(_cached_settings)
	_settings_dialog_instance.popup_centered()

func _update_model_selector(models: Array, active_model: Variant = "") -> void:
	var active_str = str(active_model) if active_model != null else ""
	var prev_selected = model_selector.get_selected_id()
	var prev_text = ""
	if model_selector.get_item_count() > 0 and prev_selected >= 0:
		prev_text = str(model_selector.get_item_metadata(prev_selected))
	model_selector.clear()
	var select_idx = 0
	for i in range(models.size()):
		var m = models[i]
		var id = m.get("id", "")
		var display_title = m.get("display_name", id)
		model_selector.add_item(display_title, i)
		model_selector.set_item_metadata(i, id)
		if (active_str != "" and id == active_str) or (prev_text != "" and id == prev_text):
			select_idx = i
	if model_selector.get_item_count() > 0:
		model_selector.select(select_idx)

func _on_bus_message_received(action: String, data: Variant) -> void:
	match action:
		"SettingsLoaded":
			print("📦 [ChatWindow] Получены настройки с бэкенда: ", JSON.stringify(data))
			if typeof(data) == TYPE_DICTIONARY:
				_cached_settings = data
				var models = data.get("models", [])
				var active = data.get("active_model", "")
				_update_model_selector(models, active)
				var lang = data.get("language", "ru")
				TranslationServer.set_locale(lang)
				print("🎯 [ChatWindow] Селектор обновлен, элементов: ", model_selector.get_item_count())
		"SetStatus":
			var status = str(data)
			_update_status_bar(status)
			var lower = status.to_lower()
			if "запрос" in lower or "request" in lower or "сборка" in lower or "печатает" in lower:
				_set_ui_loading_state(true)
			elif "готова" in lower or "ready" in lower or "ответ получен" in lower or "ошибка" in lower or "error" in lower or "таймаут" in lower or "timeout" in lower:
				_set_ui_loading_state(false)
		"StreamChunk":
			var chunk = str(data)
			_handle_stream_chunk(chunk)
		"StreamEnd":
			_handle_stream_end()
		"SetOutputBlocks":
			var blocks = data if typeof(data) == TYPE_ARRAY else []
			_render_betty_response(blocks)
			_set_ui_loading_state(false)
			input_field.grab_focus()
		"ChatHistoryLoaded":
			_handle_chat_history_loaded(data)
		"ProjectClosed":
			_clear_chat_nodes()

func _update_status_bar(status_text: String) -> void:
	if not status_bar: return
	var lower = status_text.to_lower()
	if "ошибка" in lower or "error" in lower or "сбой" in lower or "fail" in lower or "таймаут" in lower or "critical" in lower:
		status_bar.text = "❌ " + status_text
		status_bar.add_theme_color_override("font_color", Color(1.0, 0.4, 0.4))
	elif "запрос" in lower or "request" in lower or "печатает" in lower or "streaming" in lower:
		status_bar.text = "⚡ " + status_text
		status_bar.add_theme_color_override("font_color", Color(0.95, 0.75, 0.3))
	else:
		status_bar.text = "✅ " + status_text
		status_bar.add_theme_color_override("font_color", Color(0.5, 0.6, 0.7))

func _on_send_pressed() -> void:
	var user_text = input_field.text.strip_edges()
	if user_text == "": return
	_set_ui_loading_state(true)
	_spawn_text_node("User", user_text)
	var selected_model = ""
	if model_selector.item_count > 0:
		var meta = model_selector.get_item_metadata(model_selector.selected)
		if meta != null and str(meta) != "":
			selected_model = str(meta)
		else:
			selected_model = model_selector.get_item_text(model_selector.selected)
	BettyBus.send_message("SubmitInput", {
		"text": user_text,
		"model": selected_model
	})
	input_field.text = ""
	input_field.custom_minimum_size.y = 40
	_auto_scroll()

func _on_input_field_gui_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed:
		if event.keycode == KEY_ENTER:
			if event.shift_pressed:
				return
			input_field.accept_event()
			_on_send_pressed()

func _on_input_field_text_changed() -> void:
	var line_count = input_field.get_total_visible_line_count()
	var new_height = clamp(line_count * 26, 40, 150)
	input_field.custom_minimum_size.y = new_height

func _set_ui_loading_state(is_loading: bool) -> void:
	input_field.editable = !is_loading
	send_button.disabled = is_loading
	send_button.text = "⏱..." if is_loading else tr("UI_SEND_BUTTON")

func _auto_scroll() -> void:
	await get_tree().process_frame
	if scroll_container:
		var v_scroll = scroll_container.get_v_scroll_bar()
		if v_scroll:
			v_scroll.value = v_scroll.max_value

func _spawn_text_node(sender: String, text_content: String) -> void:
	var panel = PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var style = StyleBoxFlat.new()
	style.bg_color = Color(0.1, 0.1, 0.11, 1.0)
	style.border_width_left = 4
	style.set_corner_radius_all(4)
	style.set_content_margin_all(8)
	var label = RichTextLabel.new()
	label.bbcode_enabled = true
	label.fit_content = true
	label.selection_enabled = true
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	if sender == "User":
		style.border_color = Color(0.29, 0.87, 0.5, 1.0)
		label.text = "[color=#a7f3d0][b]🤠 " + tr("UI_CHAT_OPERATOR") + ":[/b][/color]\n" + text_content.strip_edges()
	else:
		style.border_color = Color(0.96, 0.62, 0.04, 1.0)
		label.text = "[color=#fde68a][b]🦊 " + tr("UI_CHAT_BETTY") + ":[/b][/color]\n" + text_content.strip_edges()
	panel.add_theme_stylebox_override("panel", style)
	panel.add_child(label)
	chat_history.add_child(panel)
	_auto_scroll()

func _spawn_code_node(title: String, code_content: String, kind_data: Variant = null) -> void:
	var panel = PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var style = StyleBoxFlat.new()
	style.bg_color = Color(0.12, 0.12, 0.14, 1.0)
	style.border_width_left = 4
	style.border_color = Color(0.3, 0.5, 0.8, 1.0)
	style.set_corner_radius_all(4)
	style.set_content_margin_all(8)
	panel.add_theme_stylebox_override("panel", style)
	var vbox = VBoxContainer.new()
	vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var top_bar = HBoxContainer.new()
	top_bar.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var label = Label.new()
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var code_to_copy = code_content
	var display_text = code_content
	if typeof(kind_data) == TYPE_DICTIONARY and kind_data.has("SearchReplace"):
		var sr = kind_data.get("SearchReplace", {})
		var target = sr.get("target", "")
		var replacement = sr.get("replacement", "")
		label.text = " 🔍 [" + tr("UI_PATCH_LABEL") + "]: " + title
		label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
		style.border_color = Color(0.2, 0.7, 0.9, 1.0)
		display_text = "--- [" + tr("UI_PATCH_FIND") + "]:\n" + target + "\n\n+++ [" + tr("UI_PATCH_REPLACE") + "]:\n" + replacement
		code_to_copy = replacement
	elif title.to_lower() == "snippet":
		label.text = " 💡 [" + tr("UI_SNIPPET_LABEL") + "]"
		label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		style.border_color = Color(0.5, 0.5, 0.5, 1.0)
	else:
		label.text = " 🛠 [" + title.to_upper() + "]"
		label.add_theme_color_override("font_color", Color(0.9, 0.6, 0.3))
	var code_edit = CodeEdit.new()
	code_edit.text = display_text.strip_edges()
	code_edit.editable = false
	code_edit.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
	code_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var line_count = code_edit.get_line_count()
	var calculated_height = clamp(line_count * 24, 80, 420)
	code_edit.custom_minimum_size = Vector2(0, calculated_height)
	code_edit.gutters_draw_line_numbers = true
	code_edit.draw_tabs = true
	code_edit.visible = false
	var is_patch = typeof(kind_data) == TYPE_DICTIONARY and kind_data.has("SearchReplace")
	var is_snippet = title.to_lower() == "snippet"
	var is_file = !is_patch and !is_snippet and title.strip_edges() != ""

	var create_btn: Button = null
	if is_file:
		create_btn = Button.new()
		create_btn.text = tr("UI_FILE_CREATE") if tr("UI_FILE_CREATE") != "UI_FILE_CREATE" else "+ Файл"
		create_btn.tooltip_text = "Создать файл в проекте: " + title
		create_btn.pressed.connect(func():
			BettyBus.send_message("CreateFile", { "relative_path": title })
			create_btn.text = "✓ " + (tr("UI_FILE_CREATED") if tr("UI_FILE_CREATED") != "UI_FILE_CREATED" else "Создан")
			var timer = get_tree().create_timer(2.0)
			timer.timeout.connect(func():
				if is_instance_valid(create_btn):
					create_btn.text = tr("UI_FILE_CREATE") if tr("UI_FILE_CREATE") != "UI_FILE_CREATE" else "+ Файл"
			)
		)

	var toggle_btn = Button.new()
	toggle_btn.text = tr("UI_CODE_EXPAND")
	toggle_btn.pressed.connect(func():
		code_edit.visible = !code_edit.visible
		toggle_btn.text = tr("UI_CODE_COLLAPSE") if code_edit.visible else tr("UI_CODE_EXPAND")
	)
	var copy_btn = Button.new()
	copy_btn.text = tr("UI_CODE_COPY")
	copy_btn.pressed.connect(func():
		DisplayServer.clipboard_set(code_to_copy)
		copy_btn.text = tr("UI_CODE_COPIED")
		var timer = get_tree().create_timer(1.5)
		timer.timeout.connect(func():
			if is_instance_valid(copy_btn):
				copy_btn.text = tr("UI_CODE_COPY")
		)
	)
	top_bar.add_child(label)
	if create_btn:
		top_bar.add_child(create_btn)
	top_bar.add_child(toggle_btn)
	top_bar.add_child(copy_btn)
	vbox.add_child(top_bar)
	vbox.add_child(code_edit)
	panel.add_child(vbox)
	chat_history.add_child(panel)
	_auto_scroll()

func _handle_stream_chunk(chunk: String) -> void:
	if _current_stream_panel == null:
		_stream_text_buffer = ""
		_current_stream_panel = PanelContainer.new()
		_current_stream_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var style = StyleBoxFlat.new()
		style.bg_color = Color(0.1, 0.1, 0.11, 1.0)
		style.border_width_left = 4
		style.border_color = Color(0.96, 0.62, 0.04, 1.0)
		style.set_corner_radius_all(4)
		style.set_content_margin_all(8)
		_current_stream_label = RichTextLabel.new()
		_current_stream_label.bbcode_enabled = true
		_current_stream_label.fit_content = true
		_current_stream_label.selection_enabled = true
		_current_stream_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_current_stream_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		_current_stream_panel.add_theme_stylebox_override("panel", style)
		_current_stream_panel.add_child(_current_stream_label)
		chat_history.add_child(_current_stream_panel)
	_stream_text_buffer += chunk
	_current_stream_label.text = "[color=#fde68a][b]🦊 " + tr("UI_CHAT_BETTY") + ":[/b][/color]\n" + _stream_text_buffer.strip_edges()
	_auto_scroll()

func _handle_stream_end() -> void:
	if _current_stream_panel != null:
		_current_stream_panel.queue_free()
		_current_stream_panel = null
		_current_stream_label = null
		_stream_text_buffer = ""

func _render_betty_response(blocks: Array) -> void:
	for block in blocks:
		if block.has("TextBlock"):
			var text_content = block.get("TextBlock", "")
			_spawn_text_node("Betty", text_content)
		elif block.has("SheetBlock"):
			var sheet = block.get("SheetBlock", {})
			var id = sheet.get("id", "Sheet")
			var content = sheet.get("content", sheet.get("raw_content", ""))
			var kind = sheet.get("kind", null)
			_spawn_code_node(id, content, kind)
	_auto_scroll()

func _clear_chat_nodes() -> void:
	for child in chat_history.get_children():
		child.queue_free()

func _handle_chat_history_loaded(data: Variant) -> void:
	_clear_chat_nodes()
	if typeof(data) != TYPE_ARRAY:
		return
	for item in data:
		if typeof(item) == TYPE_DICTIONARY:
			var user_prompt = item.get("user_prompt", "")
			if user_prompt != "":
				_spawn_text_node("User", user_prompt)
			var blocks = item.get("blocks", [])
			if typeof(blocks) == TYPE_ARRAY and not blocks.is_empty():
				_render_betty_response(blocks)
			else:
				var reply = item.get("betty_reply", "")
				if reply != "":
					_spawn_text_node("Betty", reply)
	_auto_scroll()
