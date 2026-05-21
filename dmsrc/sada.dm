#ifndef SADA
#define SADA (world.system_type == MS_WINDOWS ? "./sada.dll" : "./libsada.so")
#endif

#define SADA_CALL(func, args...) call_ext(SADA, #func)(##args)
#define SADA_CALL_BYONDAPI(func, args...) call_ext(SADA, "byond:[#func]")(##args)

/proc/sada_get_version()
	return SADA_CALL(get_version)

/proc/sada_echo(str)
	return SADA_CALL(echo, str)

/proc/sada_echo_bapi(str)
	return SADA_CALL_BYONDAPI(echo_bapi, str)

/proc/sada_panicing()
	return SADA_CALL(panicing)

/proc/sada_panicing_bapi()
	return SADA_CALL_BYONDAPI(panicing_bapi)

/proc/sada_init(path)
	return SADA_CALL(init, path)

/proc/sada_set_ptt(ckey, pressed)
	return SADA_CALL(set_ptt, ckey, pressed ? "1" : "0")

/proc/sada_update_position(mob/mob, x, y)
	return SADA_CALL_BYONDAPI(update_position, mob, x, y)

/world/New()
	. = ..()

	world.log << "Hello, world!"
	world.log << "Sada client version: [sada_get_version()]"

	var/init_result = sada_init("/tmp/sada.sock")
	var/server_version = json_decode(init_result)["version"]["version"]

	world.log << "Sada server version: [server_version]"

	world.log << "[sada_set_ptt("test_ckey", TRUE)]"
	world.log << "[sada_set_ptt("test_ckey", FALSE)]"

	world.log << "Echo test: [sada_echo("Hello, Sada!")]"

	world.log << "Byond API echo test: [sada_echo_bapi("Hello!")]"

	var/mob/test_mob = new
	test_mob.name = "Test Mob"

	for(var/i in 1 to 5)
		sada_update_position(test_mob, test_mob.x, test_mob.y)
		sleep(5)

	test_mob.something()

	shutdown()

/mob/proc/something()
	world.log << "[src]"
	return
