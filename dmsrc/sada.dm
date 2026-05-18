// #ifndef SADA
// #define SADA (world.system_type == MS_WINDOWS ? "sada.dll" : "libsada.so")
// #endif

// #define SADA_CALL(func, ...) call_ext(SADA, "[#func]")(...)

// /proc/sada_get_version()
// 	return SADA_CALL(get_version)

/world/New()
	. = ..()
	spawn(0) shutdown()
	world << "[call_ext("./libsada.so", "get_version")()]"
	// world << "SADA version: [sada_get_version()]"
