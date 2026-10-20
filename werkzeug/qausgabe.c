/* qausgabe: setzt im ISOLIERTEN headless labwc die Auflösung des (ersten)
 * Ausgangs über wlr-output-management (Testwerkzeug HMP-08/09).
 *
 *   qausgabe BREITExHÖHE
 *
 * Verweigert den Dienst in der echten Sitzung (XDG_RUNTIME_DIR der Sitzung
 * oder kein qd-runtime-Verzeichnis), genau wie qeingabe.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wayland-client.h>
#include "wlr-output-management-unstable-v1-client-protocol.h"

static struct zwlr_output_manager_v1 *manager;
static struct zwlr_output_head_v1 *head;
static uint32_t serial;
static int done, result; /* 1 ok, -1 fehlgeschlagen */

static void h_name(void *d, struct zwlr_output_head_v1 *h, const char *n) { (void)d; (void)h; fprintf(stderr, "qausgabe: Ausgang %s\n", n); }
static void h_str(void *d, struct zwlr_output_head_v1 *h, const char *s) { (void)d; (void)h; (void)s; }
static void h_size(void *d, struct zwlr_output_head_v1 *h, int32_t a, int32_t b) { (void)d; (void)h; (void)a; (void)b; }
static void h_mode(void *d, struct zwlr_output_head_v1 *h, struct zwlr_output_mode_v1 *m) { (void)d; (void)h; (void)m; }
static void h_int(void *d, struct zwlr_output_head_v1 *h, int32_t a) { (void)d; (void)h; (void)a; }
static void h_fixed(void *d, struct zwlr_output_head_v1 *h, wl_fixed_t a) { (void)d; (void)h; (void)a; }
static void h_finished(void *d, struct zwlr_output_head_v1 *h) { (void)d; (void)h; }
static const struct zwlr_output_head_v1_listener head_listener = {
	.name = h_name, .description = h_str, .physical_size = h_size, .mode = h_mode,
	.enabled = h_int, .current_mode = h_mode, .position = h_size, .transform = h_int,
	.scale = h_fixed, .finished = h_finished,
};

static void m_head(void *d, struct zwlr_output_manager_v1 *m, struct zwlr_output_head_v1 *h)
{
	(void)d; (void)m;
	zwlr_output_head_v1_add_listener(h, &head_listener, NULL);
	if (!head)
		head = h;
}
static void m_done(void *d, struct zwlr_output_manager_v1 *m, uint32_t s) { (void)d; (void)m; serial = s; done = 1; }
static void m_finished(void *d, struct zwlr_output_manager_v1 *m) { (void)d; (void)m; }
static const struct zwlr_output_manager_v1_listener manager_listener = {
	.head = m_head, .done = m_done, .finished = m_finished,
};

static void c_ok(void *d, struct zwlr_output_configuration_v1 *c) { (void)d; (void)c; result = 1; }
static void c_fail(void *d, struct zwlr_output_configuration_v1 *c) { (void)d; (void)c; result = -1; }
static const struct zwlr_output_configuration_v1_listener config_listener = {
	.succeeded = c_ok, .failed = c_fail, .cancelled = c_fail,
};

static void g_add(void *d, struct wl_registry *r, uint32_t name, const char *iface, uint32_t v)
{
	(void)d; (void)v;
	if (strcmp(iface, zwlr_output_manager_v1_interface.name) == 0) {
		manager = wl_registry_bind(r, name, &zwlr_output_manager_v1_interface, 1);
		zwlr_output_manager_v1_add_listener(manager, &manager_listener, NULL);
	}
}
static void g_remove(void *d, struct wl_registry *r, uint32_t n) { (void)d; (void)r; (void)n; }
static const struct wl_registry_listener registry_listener = { g_add, g_remove };

int main(int argc, char **argv)
{
	int w, h;
	const char *rt = getenv("XDG_RUNTIME_DIR");
	if (!rt || strncmp(rt, "/run/user/", 10) == 0 || !strstr(rt, "qd-runtime-")) {
		fprintf(stderr, "qausgabe: nicht in einer isolierten Testumgebung – verweigert\n");
		return 2;
	}
	if (argc != 2 || sscanf(argv[1], "%dx%d", &w, &h) != 2 || w < 320 || h < 240) {
		fprintf(stderr, "Aufruf: qausgabe BREITExHÖHE\n");
		return 2;
	}
	struct wl_display *display = wl_display_connect(NULL);
	if (!display) {
		fprintf(stderr, "qausgabe: keine Wayland-Verbindung\n");
		return 1;
	}
	struct wl_registry *registry = wl_display_get_registry(display);
	wl_registry_add_listener(registry, &registry_listener, NULL);
	wl_display_roundtrip(display);
	if (!manager) {
		fprintf(stderr, "qausgabe: kein zwlr_output_manager_v1\n");
		return 1;
	}
	while (!done && wl_display_dispatch(display) != -1)
		;
	if (!head) {
		fprintf(stderr, "qausgabe: kein Ausgang\n");
		return 1;
	}
	struct zwlr_output_configuration_v1 *config =
		zwlr_output_manager_v1_create_configuration(manager, serial);
	zwlr_output_configuration_v1_add_listener(config, &config_listener, NULL);
	struct zwlr_output_configuration_head_v1 *ch =
		zwlr_output_configuration_v1_enable_head(config, head);
	zwlr_output_configuration_head_v1_set_custom_mode(ch, w, h, 0);
	zwlr_output_configuration_v1_apply(config);
	while (!result && wl_display_dispatch(display) != -1)
		;
	fprintf(stderr, "qausgabe: %dx%d %s\n", w, h, result == 1 ? "gesetzt" : "ABGELEHNT");
	wl_display_disconnect(display);
	return result == 1 ? 0 : 1;
}
