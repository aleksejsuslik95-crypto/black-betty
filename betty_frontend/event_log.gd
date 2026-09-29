extends PanelContainer

func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	if has_node("/root/BettyBus"):
		BettyBus.connection_established.connect(_on_bus_connected)
		BettyBus.connection_closed.connect(_on_bus_disconnected)

func _on_bus_connected() -> void:
	BettyBus.send_message("ScanProjects")

func _on_bus_disconnected() -> void:
	pass
