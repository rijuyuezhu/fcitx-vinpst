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
  GtkWidget *entry;
  ProbeMode mode;
  const char *initial_text;
  bool require_partial;
  bool partial_seen;
  bool commit_seen;
  bool replacement_seen;
  bool timed_out;
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

static gboolean FinishWhenSuccessful(gpointer user_data) {
  Probe *probe = user_data;
  const bool partial_ok = !probe->require_partial || probe->partial_seen;
  const bool outcome_ok =
      probe->mode == PROBE_MODE_NORMAL ? probe->commit_seen : probe->replacement_seen;
  if (partial_ok && outcome_ok) {
    gtk_main_quit();
  }
  return G_SOURCE_REMOVE;
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
  EmitTextEvent("changed", text);
  if (probe->mode == PROBE_MODE_NORMAL) {
    probe->commit_seen = text != NULL && *text != '\0';
  } else if (text != NULL && *text != '\0' && strcmp(text, probe->initial_text) != 0) {
    probe->commit_seen = true;
    probe->replacement_seen = true;
  }
  g_idle_add(FinishWhenSuccessful, probe);
}

static gboolean OnTimeout(gpointer user_data) {
  Probe *probe = user_data;
  probe->timed_out = true;
  EmitTextEvent("timeout", gtk_entry_get_text(GTK_ENTRY(probe->entry)));
  gtk_main_quit();
  return G_SOURCE_REMOVE;
}

static void OnWindowDestroy(GtkWidget *widget, gpointer user_data) {
  (void)widget;
  (void)user_data;
  gtk_main_quit();
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
      .entry = NULL,
      .mode = mode,
      .initial_text = initial_text,
      .require_partial = EnvFlag("VINPUT_TOOLKIT_REQUIRE_PARTIAL", true),
      .partial_seen = false,
      .commit_seen = false,
      .replacement_seen = false,
      .timed_out = false,
  };

  GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
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
    gtk_editable_select_region(GTK_EDITABLE(probe.entry), 0, -1);
  }

  g_print("{\"event\":\"ready\",\"toolkit\":\"gtk3\",\"mode\":\"%s\","
          "\"manual_trigger\":true}\n",
          mode == PROBE_MODE_NORMAL ? "normal" : "command");
  fflush(stdout);
  g_timeout_add_seconds(TimeoutSeconds(), OnTimeout, &probe);
  gtk_main();

  const char *final_text = gtk_entry_get_text(GTK_ENTRY(probe.entry));
  const bool partial_ok = !probe.require_partial || probe.partial_seen;
  const bool outcome_ok =
      mode == PROBE_MODE_NORMAL ? probe.commit_seen : probe.replacement_seen;
  const bool ok = partial_ok && outcome_ok && !probe.timed_out;
  GString *summary = g_string_new("{\"event\":\"summary\",\"toolkit\":\"gtk3\","
                                  "\"mode\":\"");
  g_string_append(summary, mode == PROBE_MODE_NORMAL ? "normal" : "command");
  g_string_append_printf(summary,
                         "\",\"partial\":%s,\"commit\":%s,\"replacement\":%s,"
                         "\"timed_out\":%s,\"ok\":%s,\"text\":\"",
                         probe.partial_seen ? "true" : "false",
                         probe.commit_seen ? "true" : "false",
                         probe.replacement_seen ? "true" : "false",
                         probe.timed_out ? "true" : "false", ok ? "true" : "false");
  AppendJsonEscaped(summary, final_text);
  g_string_append(summary, "\"}\n");
  g_print("%s", summary->str);
  g_string_free(summary, TRUE);

  gtk_widget_destroy(window);
  return ok ? 0 : 1;
}
