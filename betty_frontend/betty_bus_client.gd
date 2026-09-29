extends Node

# Сигналы, на которые будут подписываться твои окна и кнопки
signal message_received(action: String, data: Variant)
signal connection_established()
signal connection_closed()

# Настройки подключения и процесса бэкенда
var websocket_url: String = ""
var socket: WebSocketPeer = WebSocketPeer.new()
var bus_connected: bool = false
var backend_pid: int = -1

func _ready() -> void:
	print("[BettyBus] Инициализация клиентского модуля...")
	_setup_localization()
	_start_and_connect()

func _setup_localization() -> void:
	var trans_ru = Translation.new()
	trans_ru.locale = "ru"
	var trans_en = Translation.new()
	trans_en.locale = "en"
	
	if FileAccess.file_exists("res://localization.csv"):
		var file = FileAccess.open("res://localization.csv", FileAccess.READ)
		if file:
			# Пропускаем строку заголовков (keys,ru,en)
			var _header = file.get_csv_line()
			while not file.eof_reached():
				var line = file.get_csv_line()
				if line.size() >= 3:
					var key = line[0].strip_edges()
					if key != "" and not key.begins_with("#"):
						trans_ru.add_message(key, line[1])
						trans_en.add_message(key, line[2])
			file.close()
			print("[BettyBus] Локализация успешно загружена из CSV.")
	else:
		print("[BettyBus] Внимание: localization.csv не найден, используются дефолтные строки.")
	
	TranslationServer.add_translation(trans_ru)
	TranslationServer.add_translation(trans_en)
	TranslationServer.set_locale("ru")

func _notification(what: int) -> void:
	# Когда приложение закрывается, тушим дочерний процесс Rust
	if what == NOTIFICATION_WM_CLOSE_REQUEST or what == NOTIFICATION_PREDELETE:
		_kill_backend()

func _start_and_connect() -> void:
	# 1. Запрашиваем у ОС свободный порт
	var temp_server = TCPServer.new()
	temp_server.listen(0, "127.0.0.1")
	var free_port = temp_server.get_local_port()
	temp_server.stop() # Сразу освобождаем зарезервированный порт
	
	websocket_url = "ws://127.0.0.1:" + str(free_port)
	
	# 2. Формируем путь к исполняемому файлу бэкенда
	var bin_path = OS.get_executable_path().get_base_dir().path_join("betty_backend")
	if OS.get_name() == "Windows":
		bin_path += ".exe"
		
	# Если запускаем прямо из редактора (ProjectData), ищем бинарник в корне проекта
	if OS.is_debug_build() and not FileAccess.file_exists(bin_path):
		bin_path = "res://betty_backend"
		if OS.get_name() == "Windows":
			bin_path += ".exe"
		bin_path = ProjectSettings.globalize_path(bin_path)

	# 3. Запускаем Rust-бэкенд дочерним процессом
	if FileAccess.file_exists(bin_path):
		var args = PackedStringArray(["--port", str(free_port)])
		backend_pid = OS.create_process(bin_path, args)
		print("[BettyBus] Запущен бэкенд PID: ", backend_pid, " на порту: ", free_port)
	else:
		printerr("[BettyBus] Бинарник бэкенда не найден по пути: ", bin_path)
		print("[BettyBus] Попытка подключиться к дефолтному порту (режим ручного запуска бэка)...")
		websocket_url = "ws://127.0.0.1:8765"

	# Небольшая пауза перед подключением, чтобы Rust-процесс успел инициализировать сокет
	await get_tree().create_timer(0.3).timeout
	connect_to_bus()

func _kill_backend() -> void:
	if backend_pid != -1:
		print("[BettyBus] Завершение процесса бэкенда (PID: ", backend_pid, ")...")
		OS.kill(backend_pid)
		backend_pid = -1

func _process(_delta: float) -> void:
	socket.poll() # Постоянно опрашиваем сокет в главном потоке
	
	var state = socket.get_ready_state()
	
	if state == WebSocketPeer.STATE_OPEN:
		if not bus_connected:
			bus_connected = true
			connection_established.emit()
			print("[BettyBus] Успешно подключено к центральной шине!")
		
		# Читаем входящие пакеты от бэкэнда
		while socket.get_available_packet_count() > 0:
			_handle_packet()
			
	elif state == WebSocketPeer.STATE_CLOSED:
		if bus_connected:
			bus_connected = false
			connection_closed.emit()
			print("[BettyBus] Соединение с шиной потеряно.")

func connect_to_bus() -> void:
	# Явно очищаем субпротоколы для совместимости с сырым WebSocket на Rust
	socket.supported_protocols = [] 
	
	# 🔥 Увеличиваем буферы сокета (по умолчанию они 65 КБ, ставим ~16 МБ под тяжелые контексты)
	socket.inbound_buffer_size = 16777216
	socket.outbound_buffer_size = 16777216
	
	var err = socket.connect_to_url(websocket_url)
	if err != OK:
		print("[BettyBus] Не удалось инициировать подключение: ", err)
	else:
		print("[BettyBus] Попытка подключения к ", websocket_url)

# Отправка абстракций от интерфейса к бэку
func send_message(action: String, data: Variant = null) -> void:
	socket.poll() # 🔥 Опрашиваем сокет перед отправкой
	
	if socket.get_ready_state() != WebSocketPeer.STATE_OPEN:
		print("[BettyBus] Ошибка: Попытка отправить данные в закрытый сокет!")
		return
		
	var packet: Dictionary = { "action": action }
	if data != null:
		if typeof(data) == TYPE_DICTIONARY and data.size() == 1 and (data.has("path") or data.has("focus_set")):
			if data.has("path"):
				packet["data"] = data["path"]
			elif data.has("focus_set"):
				packet["data"] = data["focus_set"]
		else:
			packet["data"] = data
	var json_string = JSON.stringify(packet)
	
	var err = socket.send_text(json_string)
	if err != OK:
		print("[BettyBus] Ошибка отправки пакета в сокет: ", err)
	else:
		print("[BettyBus] Отправлено на бэк: ", json_string)
		
	socket.poll() # 🔥 Выталкиваем данные сразу в сеть

# Парсинг ответов от бэка
func _handle_packet() -> void:
	var packet_data = socket.get_packet().get_string_from_utf8()
	var json = JSON.new()
	var error = json.parse(packet_data)
	
	if error == OK:
		if typeof(json.data) == TYPE_DICTIONARY:
			var action = json.data.get("action", "unknown")
			var data = json.data.get("data", {})
			message_received.emit(action, data)
		else:
			print("[BettyBus] Получен некорректный формат JSON")
	else:
		print("[BettyBus] Ошибка парсинга JSON: ", json.get_error_message())
