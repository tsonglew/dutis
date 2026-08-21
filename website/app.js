const associations = {
  md: { token: "MD", app: "Visual Studio Code", bundle: "com.microsoft.VSCode" },
  pdf: { token: "PDF", app: "WPS Office", bundle: "com.kingsoft.wpsoffice.mac" },
  html: { token: "HTML", app: "Microsoft Edge", bundle: "com.microsoft.edgemac" },
  png: { token: "PNG", app: "Preview", bundle: "com.apple.Preview" },
};

const tabs = [...document.querySelectorAll(".extension-tab")];
const token = document.querySelector("#file-token");
const appName = document.querySelector("#demo-app");
const bundle = document.querySelector("#demo-bundle");
const demoButton = document.querySelector("#demo-button");

function selectExtension(extension) {
  const association = associations[extension];
  if (!association) return;

  tabs.forEach((tab) => {
    const isActive = tab.dataset.extension === extension;
    tab.classList.toggle("active", isActive);
    tab.setAttribute("aria-selected", String(isActive));
  });

  [token, appName, bundle].forEach((element) => element.classList.remove("swap-in"));
  requestAnimationFrame(() => {
    token.textContent = association.token;
    appName.textContent = association.app;
    bundle.textContent = association.bundle;
    [token, appName, bundle].forEach((element) => element.classList.add("swap-in"));
  });
}

tabs.forEach((tab) => {
  tab.addEventListener("click", () => selectExtension(tab.dataset.extension));
});

demoButton.addEventListener("click", () => {
  const activeIndex = tabs.findIndex((tab) => tab.classList.contains("active"));
  const nextTab = tabs[(activeIndex + 1) % tabs.length];
  selectExtension(nextTab.dataset.extension);
});

const copyButton = document.querySelector("#copy-command");
copyButton.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(copyButton.dataset.command);
    copyButton.querySelector(".copy-label").textContent = "Copied";
    window.setTimeout(() => {
      copyButton.querySelector(".copy-label").textContent = "Copy";
    }, 1800);
  } catch {
    copyButton.querySelector(".copy-label").textContent = "Select command";
  }
});

const observer = new IntersectionObserver(
  (entries) => {
    entries.forEach((entry) => {
      if (entry.isIntersecting) {
        entry.target.classList.add("visible");
        observer.unobserve(entry.target);
      }
    });
  },
  { threshold: 0.12 },
);

document.querySelectorAll(".reveal").forEach((element) => observer.observe(element));
document.querySelector("#year").textContent = new Date().getFullYear();
