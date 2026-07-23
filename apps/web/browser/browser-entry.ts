import { platformClient, getPlatformSessionToken, setPlatformSessionToken } from "./session";
import "./session-gate.css";

type AuthenticationMode = "login" | "bootstrap";

function installMobileBrowserBehavior(isMobilePlatform: () => boolean) {
  if (!isMobilePlatform()) {
    return;
  }

  const preventGesture = (event: Event) => event.preventDefault();
  const preventPinch = (event: TouchEvent) => {
    if (event.touches.length > 1) {
      event.preventDefault();
    }
  };

  document.addEventListener("gesturestart", preventGesture, { passive: false });
  document.addEventListener("gesturechange", preventGesture, { passive: false });
  document.addEventListener("gestureend", preventGesture, { passive: false });
  document.addEventListener("touchmove", preventPinch, { passive: false });

  let rafHandle = 0;
  const setViewportHeight = () => {
    const visualViewport = window.visualViewport;
    const viewportHeight = visualViewport
      ? visualViewport.height + visualViewport.offsetTop
      : window.innerHeight;
    document.documentElement.style.setProperty(
      "--app-height",
      `${Math.round(viewportHeight)}px`,
    );
  };
  const scheduleViewportHeight = () => {
    if (rafHandle) return;
    rafHandle = window.requestAnimationFrame(() => {
      rafHandle = 0;
      setViewportHeight();
    });
  };
  const setComposerFocusState = () => {
    const activeElement = document.activeElement;
    const focused =
      activeElement instanceof HTMLTextAreaElement &&
      activeElement.closest(".composer") !== null;
    document.documentElement.dataset.mobileComposerFocus = focused ? "true" : "false";
  };

  setViewportHeight();
  setComposerFocusState();
  window.addEventListener("resize", scheduleViewportHeight, { passive: true });
  window.addEventListener("orientationchange", scheduleViewportHeight, { passive: true });
  window.visualViewport?.addEventListener("resize", scheduleViewportHeight, { passive: true });
  window.visualViewport?.addEventListener("scroll", scheduleViewportHeight, { passive: true });
  document.addEventListener("focusin", setComposerFocusState);
  document.addEventListener("focusout", () => {
    requestAnimationFrame(setComposerFocusState);
  });
}

async function renderOriginalApplication(sessionToken: string) {
  localStorage.setItem("codexMonitorWebBaseUrl", window.location.origin);
  sessionStorage.setItem("codexMonitorWebToken", sessionToken);
  const [reactModule, reactDomModule, applicationModule, platformModule] = await Promise.all([
    import("react"),
    import("react-dom/client"),
    import("../src/WebApp"),
    import("../src/utils/platformPaths"),
  ]);
  installMobileBrowserBehavior(platformModule.isMobilePlatform);
  const root = document.getElementById("root");
  if (!root) throw new Error("Missing application root");
  reactDomModule.default.createRoot(root).render(
    reactModule.default.createElement(
      reactModule.default.StrictMode,
      null,
      reactModule.default.createElement(applicationModule.default),
    ),
  );
}

function renderAuthentication(initialError: string | null = null) {
  const root = document.getElementById("root");
  if (!root) throw new Error("Missing application root");

  let mode: AuthenticationMode = "login";
  const render = (error: string | null = initialError) => {
    root.replaceChildren();
    const main = document.createElement("main");
    main.className = "platform-session-gate";
    const card = document.createElement("section");
    card.className = "platform-session-card";

    const eyebrow = document.createElement("p");
    eyebrow.className = "platform-session-eyebrow";
    eyebrow.textContent = "OPEN WEB CODEX";
    const title = document.createElement("h1");
    title.textContent = "Self-hosted Codex workbench";
    const description = document.createElement("p");
    description.textContent = "Sign in to your isolated Profile and authorized Git workspaces.";
    card.append(eyebrow, title, description);

    if (error) {
      const errorBox = document.createElement("div");
      errorBox.className = "platform-session-error";
      errorBox.setAttribute("role", "alert");
      errorBox.textContent = error;
      card.append(errorBox);
    }

    const form = document.createElement("form");
    if (mode === "bootstrap") {
      const name = document.createElement("input");
      name.name = "name";
      name.placeholder = "Your name";
      name.required = true;
      form.append(name);
    }
    const username = document.createElement("input");
    username.name = "username";
    username.autocomplete = "username";
    username.placeholder = "Username";
    username.required = true;
    form.append(username);
    if (mode === "bootstrap") {
      const email = document.createElement("input");
      email.name = "email";
      email.type = "email";
      email.autocomplete = "email";
      email.placeholder = "Email";
      email.required = true;
      form.append(email);
    }
    const password = document.createElement("input");
    password.name = "password";
    password.type = "password";
    password.autocomplete = mode === "login" ? "current-password" : "new-password";
    password.placeholder = "Password";
    password.required = true;
    const submit = document.createElement("button");
    submit.type = "submit";
    submit.textContent = mode === "login" ? "Sign in" : "Initialize instance";
    form.append(password, submit);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const fields = new FormData(form);
      submit.disabled = true;
      const request = mode === "bootstrap"
        ? platformClient.bootstrap(
            String(fields.get("name") ?? ""),
            String(fields.get("username") ?? ""),
            String(fields.get("email") ?? ""),
            String(fields.get("password") ?? ""),
          )
        : platformClient.login(
            String(fields.get("username") ?? ""),
            String(fields.get("password") ?? ""),
          );
      void request.then((session) => {
        setPlatformSessionToken(session.session_token);
        window.location.reload();
      }).catch((reason) => {
        render(reason instanceof Error ? reason.message : String(reason));
      });
    });
    card.append(form);

    const toggle = document.createElement("button");
    toggle.className = "platform-session-switch";
    toggle.type = "button";
    toggle.textContent = mode === "login"
      ? "First run? Initialize the instance"
      : "Already initialized? Sign in";
    toggle.addEventListener("click", () => {
      mode = mode === "login" ? "bootstrap" : "login";
      render(null);
    });
    card.append(toggle);
    main.append(card);
    root.append(main);
  };

  render(initialError);
}

async function start() {
  const token = getPlatformSessionToken();
  if (!token) {
    renderAuthentication();
    return;
  }
  try {
    await platformClient.me();
    await renderOriginalApplication(token);
  } catch (reason) {
    setPlatformSessionToken("");
    renderAuthentication(reason instanceof Error ? reason.message : String(reason));
  }
}

void start();
