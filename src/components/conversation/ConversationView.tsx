import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import { useSettings } from "../../hooks/useSettings";
import { commands } from "@/bindings";

interface Message {
  role: "user" | "assistant";
  content: string;
}

/**
 * Live transcript of the Speech → Brain → Speech loop.
 *
 * Turns arrive either from the converse hotkey (speech) or the text input
 * below; the assistant reply streams in via brain:* events and is spoken
 * aloud when read-aloud is enabled.
 */
export const ConversationView: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState("");
  const [streaming, setStreaming] = useState("");
  const [thinking, setThinking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  const brainEnabled = settings?.brain?.enabled ?? false;
  const converseBinding = settings?.bindings?.converse?.current_binding;

  useEffect(() => {
    const setup = async () => {
      const unlistenThinking = await listen("brain:thinking", () => {
        setThinking(true);
        setStreaming("");
        setError(null);
      });
      const unlistenToken = await listen<string>("brain:token", (event) => {
        setThinking(false);
        setStreaming((prev) => prev + event.payload);
      });
      const unlistenDone = await listen<string>("brain:done", (event) => {
        setThinking(false);
        setStreaming("");
        setMessages((prev) => [
          ...prev,
          { role: "assistant", content: event.payload },
        ]);
      });
      const unlistenError = await listen<string>("brain:error", (event) => {
        setThinking(false);
        setStreaming("");
        setError(event.payload);
      });
      // Speech turns: surface the transcribed question in the transcript.
      const unlistenAsked = await listen<string>("brain:asked", (event) => {
        setMessages((prev) => [
          ...prev,
          { role: "user", content: event.payload },
        ]);
      });
      return () => {
        unlistenThinking();
        unlistenToken();
        unlistenDone();
        unlistenError();
        unlistenAsked();
      };
    };
    const cleanup = setup();
    return () => {
      void cleanup.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streaming, thinking]);

  const send = useCallback(async () => {
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    setError(null);
    // brain:asked covers speech turns; text turns are added locally.
    setMessages((prev) => [...prev, { role: "user", content: text }]);
    const result = await commands.brainAsk(text);
    if (result.status === "error") {
      setError(String(result.error));
    }
  }, [draft]);

  const clear = useCallback(async () => {
    await commands.brainClearHistory();
    setMessages([]);
    setStreaming("");
    setError(null);
  }, []);

  return (
    <div className="flex flex-col h-full min-h-[400px] space-y-3">
      {!brainEnabled && (
        <div className="px-4 py-3 rounded-lg border border-mid-gray/20 text-sm text-mid-gray">
          {t("conversation.disabledHint")}
        </div>
      )}
      {brainEnabled && converseBinding && (
        <div className="px-4 py-2 rounded-lg border border-mid-gray/20 text-xs text-mid-gray">
          {t("conversation.hotkeyHint", { hotkey: converseBinding })}
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-3 px-1">
        {messages.length === 0 && !streaming && !thinking && (
          <p className="text-sm text-mid-gray px-3 py-6 text-center">
            {t("conversation.empty")}
          </p>
        )}
        {messages.map((message, i) => (
          <div
            key={i}
            className={`max-w-[85%] rounded-lg px-3 py-2 text-sm whitespace-pre-wrap ${
              message.role === "user"
                ? "bg-logo-primary/20 ms-auto"
                : "bg-mid-gray/10"
            }`}
          >
            {message.content}
          </div>
        ))}
        {thinking && (
          <div className="max-w-[85%] rounded-lg px-3 py-2 text-sm bg-mid-gray/10 animate-pulse">
            {t("conversation.thinking")}
          </div>
        )}
        {streaming && (
          <div className="max-w-[85%] rounded-lg px-3 py-2 text-sm bg-mid-gray/10 whitespace-pre-wrap">
            {streaming}
          </div>
        )}
        {error && (
          <div className="max-w-[85%] rounded-lg px-3 py-2 text-sm text-red-500 border border-red-500/30">
            {error}
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      <div className="space-y-2">
        <Textarea
          variant="compact"
          rows={2}
          value={draft}
          placeholder={t("conversation.inputPlaceholder")}
          disabled={!brainEnabled}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <div className="flex gap-2">
          <Button
            variant="primary-soft"
            size="sm"
            disabled={!brainEnabled || !draft.trim()}
            onClick={() => void send()}
          >
            {t("conversation.send")}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void commands.brainAbort()}
          >
            {t("conversation.stop")}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => void clear()}>
            {t("conversation.clear")}
          </Button>
        </div>
      </div>
    </div>
  );
};
