#ifndef SADA
#define SADA (world.system_type == MS_WINDOWS ? "./sada.dll" : "./libsada.so")
#endif

#define SADA_CALL(func, args...) call_ext(SADA, #func)(##args)
#define SADA_CALL_BYONDAPI(func, args...) call_ext(SADA, "byond:[#func]")(##args)

/proc/sada_get_version()
	return SADA_CALL(get_version)

/proc/sada_echo(str)
	return SADA_CALL(echo, str)

/proc/sada_echo2(str)
	return SADA_CALL_BYONDAPI(echo2, str)

/proc/sada_echo3(str)
	return SADA_CALL_BYONDAPI(echo3, str)

/proc/sada_panicing()
	return SADA_CALL(panicing)

/proc/sada_panicing2()
	return SADA_CALL_BYONDAPI(panicing2)

/proc/sada_init(path)
	return SADA_CALL(init, path)

/proc/sada_set_ptt(ckey, pressed)
	return SADA_CALL(set_ptt, ckey, pressed ? "1" : "0")

/world/New()
	. = ..()

	spawn(0) shutdown()

	world.log << "Hello, world!"
	world.log << "Sada client version: [sada_get_version()]"

	var/init_result = sada_init("/tmp/sada.sock")
	var/server_version = json_decode(init_result)["version"]["version"]

	world.log << "Sada server version: [server_version]"

	world.log << "[sada_set_ptt("test_ckey", TRUE)]"
	world.log << "[sada_set_ptt("test_ckey", FALSE)]"

	world.log << "Echo test: [sada_echo("Hello, Sada!")]"

	world.log << "Byond API echo test: [sada_echo2("Hello, Byond API!")]"

	world.log << "Byond API echo test 2: [sada_echo3("Hello, Byond API 2!")]"

	sada_panicing2()
