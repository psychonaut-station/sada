#ifndef SADA
#define SADA (world.system_type == MS_WINDOWS ? "./sada.dll" : "./libsada.so")
#endif

#define SADA_CALL(func, args...) call_ext(SADA, #func)(##args)

/proc/sada_get_version()
	return SADA_CALL(get_version)

/proc/sada_echo(str)
	return SADA_CALL(echo, str)

/proc/sada_panicing()
	return SADA_CALL(panicing)

/proc/sada_init(path)
	return SADA_CALL(init, path)

/proc/sada_set_ptt(ckey, pressed)
	return SADA_CALL(set_ptt, ckey, pressed ? "1" : "0")

/world/New()
	. = ..()

	world.log << "Hello, world!"
	world.log << "Sada version: [sada_get_version()]"

	sada_init("/tmp/sada.sock")
	sada_set_ptt("test_ckey", TRUE)
	sada_set_ptt("test_ckey", FALSE)

	world.log << "Echo test: [sada_echo("Hello, Sada!")]"
