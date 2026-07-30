#include <gtk/gtk.h>

#include <errno.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

typedef enum {
  PROBE_MODE_NORMAL,
  PROBE_MODE_COMMAND,
} ProbeMode;

typedef struct {
  GtkWidget *window;
  GtkWidget *entry;
  GDBusConnection *bus;
  char *last_text;
  guint partial_subscription_id;
  guint selection_source_id;
  guint finish_source_id;
  guint timeout_source_id;
  ProbeMode mode;
  const char *initial_text;
  const char *expected_commit_substring;
  bool require_partial;
  bool partial_seen;
  bool commit_seen;
  bool replacement_seen;
  bool selection_ready;
  bool timed_out;
  bool window_destroyed;
} Probe;

static void AppendJsonEscaped(GString *output, const char *text) {
  const unsigned char *cursor = (const unsigned char *)(text == NULL ? "" : text);
  while (*cursor != '\0') {
    switch (*cursor) {
    case '\\':
      g_string_append(output, "\\\\");
      break;
    case '"':
      g_string_append(output, "\\\"");
      break;
    case '\n':
      g_string_append(output, "\\n");
      break;
    case '\r':
      g_string_append(output, "\\r");
      break;
    case '\t':
      g_string_append(output, "\\t");
      break;
    default:
      if (*cursor < 0x20) {
        g_string_append_printf(output, "\\u%04x", *cursor);
      } else {
        g_string_append_c(output, (char)*cursor);
      }
      break;
    }
    ++cursor;
  }
}

static void EmitTextEvent(const char *event, const char *text) {
  GString *line = g_string_new("{\"event\":\"");
  AppendJsonEscaped(line, event);
  g_string_append(line, "\",\"text\":\"");
  AppendJsonEscaped(line, text);
  g_string_append(line, "\"}\n");
  g_print("%s", line->str);
  fflush(stdout);
  g_string_free(line, TRUE);
}

static unsigned int TimeoutSeconds(void) {
  const char *value = g_getenv("VINPUT_TOOLKIT_TIMEOUT_SECONDS");
  if (value == NULL || *value == '\0') {
    return 60;
  }
  errno = 0;
  char *end = NULL;
  const unsigned long parsed = strtoul(value, &end, 10);
  if (errno != 0 || end == value || *end != '\0' || parsed == 0 || parsed > 3600) {
    g_printerr("invalid VINPUT_TOOLKIT_TIMEOUT_SECONDS: %s\n", value);
    return 60;
  }
  return (unsigned int)parsed;
}

static bool EnvFlag(const char *name, bool fallback) {
  const char *value = g_getenv(name);
  if (value == NULL || *value == '\0') {
    return fallback;
  }
  return strcmp(value, "0") != 0 && g_ascii_strcasecmp(value, "false") != 0 &&
         g_ascii_strcasecmp(value, "no") != 0;
}

static bool DaemonIsRecording(Probe *probe) {
  if (probe->bus == NULL) {
    return true;
  }

  GError *error = NULL;
  GVariant *reply = g_dbus_connection_call_sync(
      probe->bus, "org.fcitx.Vinput", "/org/fcitx/Vinput", "org.fcitx.Vinput.Service",
      "GetStatus", NULL, G_VARIANT_TYPE("(s)"), G_DBUS_CALL_FLAGS_NONE, 1000, NULL,
      &error);
  if (reply == NULL) {
    g_clear_error(&error);
    return true;
  }

  const char *status = NULL;
  g_variant_get(reply, "(&s)", &status);
  const bool recording = status != NULL && strcmp(status, "recording") == 0;
  g_variant_unref(reply);
  return recording;
}

static void RememberText(Probe *probe, const char *text) {
  g_free(probe->last_text);
  probe->last_text = g_strdup(text == NULL ? "" : text);
}

static void QuitMainLoop(void) {
  if (gtk_main_level() > 0) {
    gtk_main_quit();
  }
}

static void OnRecognitionPartial(GDBusConnection *connection, const gchar *sender_name,
                                 const gchar *object_path, const gchar *interface_name,
                                 const gchar *signal_name, GVariant *parameters,
                                 gpointer user_data) {
  (void)connection;
  (void)sender_name;
  (void)object_path;
  (void)interface_name;
  (void)signal_name;
  Probe *probe = user_data;
  const char *partial = NULL;
  g_variant_get(parameters, "(&s)", &partial);
  EmitTextEvent("daemon-partial", partial);
  if (partial != NULL && *partial != '\0') {
    probe->partial_seen = true;
  }
}

static void EmitReady(const Probe *probe) {
  g_print("{\"event\":\"ready\",\"toolkit\":\"gtk3\",\"mode\":\"%s\","
          "\"manual_trigger\":true}\n",
          probe->mode == PROBE_MODE_NORMAL ? "normal" : "command");
  fflush(stdout);
}

static gboolean PrepareCommandSelection(gpointer user_data) {
  Probe *probe = user_data;
  if (probe->window_destroyed || probe->entry == NULL) {
    probe->selection_source_id = 0;
    return G_SOURCE_REMOVE;
  }
  if (!gtk_widget_has_focus(probe->entry)) {
    return G_SOURCE_CONTINUE;
  }

  gtk_editable_select_region(GTK_EDITABLE(probe->entry), 0, -1);
  gint start = 0;
  gint end = 0;
  if (!gtk_editable_get_selection_bounds(GTK_EDITABLE(probe->entry), &start, &end) ||
      start == end) {
    return G_SOURCE_CONTINUE;
  }
  char *selected = gtk_editable_get_chars(GTK_EDITABLE(probe->entry), start, end);
  const bool matches = selected != NULL && strcmp(selected, probe->initial_text) == 0;
  EmitTextEvent("selection-ready", selected);
  g_free(selected);
  if (!matches) {
    return G_SOURCE_CONTINUE;
  }

  probe->selection_ready = true;
  probe->selection_source_id = 0;
  EmitReady(probe);
  return G_SOURCE_REMOVE;
}

static void UpdateFinalOutcome(Probe *probe) {
  if (DaemonIsRecording(probe) || probe->last_text == NULL ||
      *probe->last_text == '\0') {
    return;
  }
  if (probe->mode == PROBE_MODE_NORMAL) {
    probe->commit_seen = true;
    return;
  }
  if (strcmp(probe->last_text, probe->initial_text) != 0) {
    probe->commit_seen = true;
    probe->replacement_seen = true;
  }
}

static gboolean FinishWhenSuccessful(gpointer user_data) {
  Probe *probe = user_data;
  UpdateFinalOutcome(probe);
  const bool partial_ok = !probe->require_partial || probe->partial_seen;
  const bool selection_ok = probe->mode == PROBE_MODE_NORMAL || probe->selection_ready;
  const bool expected_commit_ok =
      probe->expected_commit_substring == NULL ||
      *probe->expected_commit_substring == '\0' ||
      (probe->last_text != NULL &&
       strstr(probe->last_text, probe->expected_commit_substring) != NULL);
  const bool outcome_ok =
      probe->mode == PROBE_MODE_NORMAL ? probe->commit_seen : probe->replacement_seen;
  if (partial_ok && selection_ok && expected_commit_ok && outcome_ok) {
    probe->finish_source_id = 0;
    QuitMainLoop();
    return G_SOURCE_REMOVE;
  }
  return G_SOURCE_CONTINUE;
}

static void OnPreeditChanged(GtkEntry *entry, const gchar *preedit,
                             gpointer user_data) {
  (void)entry;
  Probe *probe = user_data;
  EmitTextEvent("preedit", preedit);
  if (preedit != NULL && *preedit != '\0' && strstr(preedit, "...") == NULL) {
    probe->partial_seen = true;
  }
}

static void OnChanged(GtkEditable *editable, gpointer user_data) {
  Probe *probe = user_data;
  const char *text = gtk_entry_get_text(GTK_ENTRY(editable));
  RememberText(probe, text);
  EmitTextEvent("changed", text);
  if (text != NULL && *text != '\0' && DaemonIsRecording(probe)) {
    probe->partial_seen = true;
  }
}

static gboolean OnTimeout(gpointer user_data) {
  Probe *probe = user_data;
  probe->timeout_source_id = 0;
  probe->timed_out = true;
  EmitTextEvent("timeout", probe->last_text);
  QuitMainLoop();
  return G_SOURCE_REMOVE;
}

static void OnWindowDestroy(GtkWidget *widget, gpointer user_data) {
  (void)widget;
  Probe *probe = user_data;
  probe->window = NULL;
  probe->entry = NULL;
  probe->window_destroyed = true;
  QuitMainLoop();
}

static ProbeMode ParseMode(const char *value) {
  if (strcmp(value, "normal") == 0) {
    return PROBE_MODE_NORMAL;
  }
  if (strcmp(value, "command") == 0) {
    return PROBE_MODE_COMMAND;
  }
  g_printerr("mode must be `normal` or `command`\n");
  exit(2);
}

int main(int argc, char **argv) {
  if (argc != 2) {
    g_printerr("usage: %s normal|command\n", argv[0]);
    return 2;
  }
  gtk_init(&argc, &argv);

  const ProbeMode mode = ParseMode(argv[1]);
  const char *initial_text = g_getenv("VINPUT_TOOLKIT_INITIAL_TEXT");
  if (initial_text == NULL || *initial_text == '\0') {
    initial_text = "selected text";
  }
  Probe probe = {
      .window = NULL,
      .entry = NULL,
      .bus = NULL,
      .last_text = g_strdup(mode == PROBE_MODE_COMMAND ? initial_text : ""),
      .partial_subscription_id = 0,
      .selection_source_id = 0,
      .finish_source_id = 0,
      .timeout_source_id = 0,
      .mode = mode,
      .initial_text = initial_text,
      .expected_commit_substring = g_getenv("VINPUT_TOOLKIT_EXPECTED_COMMIT_SUBSTRING"),
      .require_partial = EnvFlag("VINPUT_TOOLKIT_REQUIRE_PARTIAL", true),
      .partial_seen = false,
      .commit_seen = false,
      .replacement_seen = false,
      .selection_ready = mode == PROBE_MODE_NORMAL,
      .timed_out = false,
      .window_destroyed = false,
  };

  GError *bus_error = NULL;
  probe.bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &bus_error);
  if (probe.bus == NULL) {
    g_printerr("failed to connect to the session bus: %s\n",
               bus_error == NULL ? "unknown error" : bus_error->message);
    g_clear_error(&bus_error);
    g_free(probe.last_text);
    return 1;
  }
  probe.partial_subscription_id = g_dbus_connection_signal_subscribe(
      probe.bus, "org.fcitx.Vinput", "org.fcitx.Vinput.Service", "RecognitionPartial",
      "/org/fcitx/Vinput", NULL, G_DBUS_SIGNAL_FLAGS_NONE, OnRecognitionPartial, &probe,
      NULL);

  GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
  probe.window = window;
  gtk_window_set_default_size(GTK_WINDOW(window), 640, 140);
  gtk_window_set_title(GTK_WINDOW(window), "fcitx-vinput GTK3 live probe");
  g_signal_connect(window, "destroy", G_CALLBACK(OnWindowDestroy), &probe);

  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
  gtk_container_set_border_width(GTK_CONTAINER(box), 16);
  gtk_container_add(GTK_CONTAINER(window), box);

  const char *instruction =
      mode == PROBE_MODE_NORMAL
          ? "Focus the field, press the normal dictation key, speak, then press it "
            "again."
          : "The text is selected. Press the command dictation key, speak, stop, and "
            "choose the replacement candidate.";
  GtkWidget *label = gtk_label_new(instruction);
  gtk_label_set_line_wrap(GTK_LABEL(label), TRUE);
  gtk_label_set_xalign(GTK_LABEL(label), 0.0F);
  gtk_box_pack_start(GTK_BOX(box), label, FALSE, FALSE, 0);

  probe.entry = gtk_entry_new();
  if (mode == PROBE_MODE_COMMAND) {
    gtk_entry_set_text(GTK_ENTRY(probe.entry), initial_text);
    gtk_editable_select_region(GTK_EDITABLE(probe.entry), 0, -1);
  }
  g_signal_connect(probe.entry, "preedit-changed", G_CALLBACK(OnPreeditChanged),
                   &probe);
  g_signal_connect(probe.entry, "changed", G_CALLBACK(OnChanged), &probe);
  gtk_box_pack_start(GTK_BOX(box), probe.entry, FALSE, FALSE, 0);

  gtk_widget_show_all(window);
  gtk_widget_grab_focus(probe.entry);
  if (mode == PROBE_MODE_COMMAND) {
    probe.selection_source_id = g_timeout_add(100, PrepareCommandSelection, &probe);
  } else {
    EmitReady(&probe);
  }

  probe.finish_source_id = g_timeout_add(200, FinishWhenSuccessful, &probe);
  probe.timeout_source_id = g_timeout_add_seconds(TimeoutSeconds(), OnTimeout, &probe);
  gtk_main();

  if (probe.timeout_source_id != 0) {
    g_source_remove(probe.timeout_source_id);
    probe.timeout_source_id = 0;
  }
  if (probe.finish_source_id != 0) {
    g_source_remove(probe.finish_source_id);
    probe.finish_source_id = 0;
  }
  if (probe.selection_source_id != 0) {
    g_source_remove(probe.selection_source_id);
    probe.selection_source_id = 0;
  }
  UpdateFinalOutcome(&probe);
  const char *final_text = probe.last_text;
  const bool partial_ok = !probe.require_partial || probe.partial_seen;
  const bool selection_ok = mode == PROBE_MODE_NORMAL || probe.selection_ready;
  const bool expected_commit_ok =
      probe.expected_commit_substring == NULL ||
      *probe.expected_commit_substring == '\0' ||
      (final_text != NULL &&
       strstr(final_text, probe.expected_commit_substring) != NULL);
  const bool outcome_ok =
      mode == PROBE_MODE_NORMAL ? probe.commit_seen : probe.replacement_seen;
  const bool ok = partial_ok && selection_ok && expected_commit_ok && outcome_ok &&
                  !probe.timed_out;
  GString *summary = g_string_new("{\"event\":\"summary\",\"toolkit\":\"gtk3\","
                                  "\"mode\":\"");
  g_string_append(summary, mode == PROBE_MODE_NORMAL ? "normal" : "command");
  g_string_append_printf(
      summary,
      "\",\"partial\":%s,\"commit\":%s,\"replacement\":%s,"
      "\"selection_ready\":%s,\"expected_commit\":%s,"
      "\"timed_out\":%s,\"ok\":%s,\"text\":\"",
      probe.partial_seen ? "true" : "false", probe.commit_seen ? "true" : "false",
      probe.replacement_seen ? "true" : "false",
      probe.selection_ready ? "true" : "false", expected_commit_ok ? "true" : "false",
      probe.timed_out ? "true" : "false", ok ? "true" : "false");
  AppendJsonEscaped(summary, final_text);
  g_string_append(summary, "\"}\n");
  g_print("%s", summary->str);
  g_string_free(summary, TRUE);

  if (!probe.window_destroyed && probe.window != NULL) {
    gtk_widget_destroy(probe.window);
  }
  if (probe.partial_subscription_id != 0) {
    g_dbus_connection_signal_unsubscribe(probe.bus, probe.partial_subscription_id);
  }
  g_object_unref(probe.bus);
  g_free(probe.last_text);
  return ok ? 0 : 1;
}
