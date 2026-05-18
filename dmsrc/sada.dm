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

/world/New()
	. = ..()
	spawn(0) shutdown()
	world.log << "Hello, world!"
	world.log << "Sada version: [sada_get_version()]"
	sada_panicing()
	world.log << "Echo test: [sada_echo("Hello, Sada!")]"
