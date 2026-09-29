extends Window

signal settings_saved()

@onready var gemini_key_edit: LineEdit = %GeminiKeyEdit
@onready var openrouter_key_edit: LineEdit = %OpenRouterKeyEdit
@onready var deepseek_key_edit: LineEdit = %DeepSeekKeyEdit
@onready var openai_key_edit: LineEdit = %OpenAiKeyEdit

@onready var models_tree: Tree = %ModelsTree
@onready var new_model_id: LineEdit = %NewModelId
@onready var new_model_name: LineEdit = %NewModelName
@onready var new_model_provider: OptionButton = %NewModelProvider
@onready var new_model_url: LineEdit = %NewModelUrl
@onready var new_model_key_name: OptionButton = %NewModelKeyName

@onready var add_model_btn: Button = %AddModelBtn
@onready var remove_model_btn: Button = %RemoveModelBtn
@onready var save_btn: Button = %SaveBtn
@onready var cancel_btn: Button = %CancelBtn
@onready var language_select: OptionButton = %LanguageSelect
var _current_settings: Dictionary = {
	"api_keys": {},
	"models": [],
	"active_model": "",
	"language": "ru"
}
func _ready() -> void:
	close_requested.connect(_on_cancel_pressed)
	cancel_btn.pressed.connect(_on_cancel_pressed)
	save_btn.pressed.connect(_on_save_pressed)
	add_model_btn.pressed.connect(_on_add_model_pressed)
	remove_model_btn.pressed.connect(_on_remove_model_pressed)
	
	if language_select:
		language_select.clear()
		language_select.add_item("🇷🇺 Русский (RU)", 0)
		language_select.set_item_metadata(0, "ru")
		language_select.add_item("🇬🇧 English (EN)", 1)
		language_select.set_item_metadata(1, "en")
		language_select.item_selected.connect(_on_language_selected)
	
	new_model_provider.clear()
	new_model_provider.add_item("Gemini Native", 0)
	new_model_provider.add_item("OpenAI-Compatible", 1)
	new_model_provider.item_selected.connect(_on_provider_selected)
	new_model_key_name.clear()
	new_model_key_name.add_item("gemini", 0)
	new_model_key_name.add_item("openrouter", 1)
	new_model_key_name.add_item("deepseek", 2)
	new_model_key_name.add_item("openai", 3)
	new_model_key_name.add_item(tr("UI_SETTINGS_NO_KEY_OPTION"), 4)
	_setup_tree()
	_on_provider_selected(0)

func _on_language_selected(idx: int) -> void:
	var lang = str(language_select.get_item_metadata(idx))
	TranslationServer.set_locale(lang)
	_current_settings["language"] = lang
	BettyBus.send_message("SetLocale", lang)
	title = tr("UI_SETTINGS_TITLE")

func _notification(what: int) -> void:
	if what == NOTIFICATION_TRANSLATION_CHANGED:
		if not is_node_ready():
			return
		title = tr("UI_SETTINGS_TITLE")
		_setup_tree()
func _setup_tree() -> void:
	if not models_tree:
		return
	models_tree.columns = 4
	models_tree.set_column_title(0, tr("UI_SETTINGS_COL_NAME"))
	models_tree.set_column_title(1, tr("UI_SETTINGS_COL_ID"))
	models_tree.set_column_title(2, tr("UI_SETTINGS_COL_PROTOCOL"))
	models_tree.set_column_title(3, tr("UI_SETTINGS_COL_KEY"))
	models_tree.column_titles_visible = true
	models_tree.hide_root = true

func populate_from_settings(settings: Dictionary) -> void:
	_current_settings = settings.duplicate(true)
	var keys = _current_settings.get("api_keys", {})
	gemini_key_edit.text = keys.get("gemini", "")
	openrouter_key_edit.text = keys.get("openrouter", "")
	deepseek_key_edit.text = keys.get("deepseek", "")
	openai_key_edit.text = keys.get("openai", "")
	
	var lang = _current_settings.get("language", "ru")
	if language_select:
		if lang == "en":
			language_select.select(1)
		else:
			language_select.select(0)
	TranslationServer.set_locale(lang)
	title = tr("UI_SETTINGS_TITLE")
	_rebuild_models_tree()

func _rebuild_models_tree() -> void:
	models_tree.clear()
	var root = models_tree.create_item()
	var models = _current_settings.get("models", [])

	for model in models:
		var item = models_tree.create_item(root)
		item.set_text(0, model.get("display_name", ""))
		item.set_text(1, model.get("id", ""))
		var provider = model.get("provider", "Gemini")
		item.set_text(2, str(provider))
		item.set_text(3, model.get("api_key_name", ""))
		item.set_metadata(0, model)

func _on_provider_selected(idx: int) -> void:
	if idx == 0:
		new_model_url.editable = false
		new_model_url.placeholder_text = "Google API (встроено)"
		new_model_url.text = ""
	else:
		new_model_url.editable = true
		new_model_url.placeholder_text = "https://openrouter.ai/api/v1/chat/completions или localhost:11434"

func _on_add_model_pressed() -> void:
	var m_id = new_model_id.text.strip_edges()
	var m_name = new_model_name.text.strip_edges()
	if m_id.is_empty():
		return
	if m_name.is_empty():
		m_name = m_id

	var provider_str = "Gemini" if new_model_provider.selected == 0 else "OpenAiCompatible"
	var key_choice = new_model_key_name.get_item_text(new_model_key_name.selected)
	if key_choice.begins_with("<"):
		key_choice = ""

	var url_val = null
	if provider_str == "OpenAiCompatible":
		var typed_url = new_model_url.text.strip_edges()
		if not typed_url.is_empty():
			url_val = typed_url

	var new_entry = {
		"id": m_id,
		"display_name": m_name,
		"provider": provider_str,
		"base_url": url_val,
		"api_key_name": key_choice
	}

	var list: Array = _current_settings.get("models", [])
	list.append(new_entry)
	_current_settings["models"] = list

	new_model_id.text = ""
	new_model_name.text = ""
	new_model_url.text = ""

	_rebuild_models_tree()

func _on_remove_model_pressed() -> void:
	var selected = models_tree.get_selected()
	if not selected:
		return

	var data = selected.get_metadata(0)
	if data:
		var list: Array = _current_settings.get("models", [])
		var target_id = data.get("id", "")
		var filtered = []
		for item in list:
			if item.get("id") != target_id:
				filtered.append(item)
		_current_settings["models"] = filtered
		_rebuild_models_tree()

func _on_save_pressed() -> void:
	var keys = {
		"gemini": gemini_key_edit.text.strip_edges(),
		"openrouter": openrouter_key_edit.text.strip_edges(),
		"deepseek": deepseek_key_edit.text.strip_edges(),
		"openai": openai_key_edit.text.strip_edges()
	}
	_current_settings["api_keys"] = keys

	BettyBus.send_message("SaveSettings", _current_settings)
	settings_saved.emit()
	hide()

func _on_cancel_pressed() -> void:
	hide()
