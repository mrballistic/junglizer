// Minimal analytics consent, ported from the RAID Calculator site. Google's
// tag is not requested at all until the visitor accepts; declining (or never
// answering) means no Google request and no cookies. The choice is kept in
// localStorage as "granted" or "denied", and any element with
// [data-consent-open] reopens the banner so it can be changed later.
(() => {
  const GA_ID = "G-CWEHQPGMV8";
  const KEY = "junglizer-analytics-consent";
  // Resolve the privacy page relative to this script, so the same file works
  // from the home page and from /privacy/.
  const privacyHref = new URL("../privacy/", document.currentScript.src).href;

  let loaded = false;
  function loadAnalytics() {
    if (loaded) return;
    loaded = true;
    const tag = document.createElement("script");
    tag.async = true;
    tag.src = "https://www.googletagmanager.com/gtag/js?id=" + GA_ID;
    document.head.appendChild(tag);
    window.dataLayer = window.dataLayer || [];
    window.gtag = function () { dataLayer.push(arguments); };
    gtag("js", new Date());
    gtag("config", GA_ID);
  }

  function read() {
    try { return localStorage.getItem(KEY); } catch (e) { return null; }
  }
  function save(value) {
    try { localStorage.setItem(KEY, value); } catch (e) { /* private mode: ask again next visit */ }
  }

  let banner = null;
  let opener = null;
  function showBanner(focus) {
    if (!banner) {
      banner = document.createElement("section");
      banner.className = "consent";
      banner.setAttribute("aria-label", "Analytics cookies");
      banner.innerHTML =
        '<p>This site uses Google Analytics cookies to count visits. Junglizer itself collects nothing. <a href="' + privacyHref + '">Privacy policy</a></p>' +
        '<div class="consent-actions">' +
        '<button type="button" data-choice="denied">Decline</button>' +
        '<button type="button" data-choice="granted">Accept</button>' +
        "</div>";
      banner.addEventListener("click", (event) => {
        const choice = event.target.closest("[data-choice]")?.dataset.choice;
        if (!choice) return;
        const previous = read();
        save(choice);
        banner.hidden = true;
        // Return focus to the control that reopened the banner, if any.
        if (opener && document.contains(opener)) opener.focus();
        opener = null;
        if (choice === "granted") loadAnalytics();
        // Withdrawing consent after the tag has loaded: reload so it's gone.
        else if (previous === "granted" && loaded) location.reload();
      });
      document.body.appendChild(banner);
    }
    banner.hidden = false;
    if (focus) banner.querySelector("button").focus();
  }

  const choice = read();
  if (choice === "granted") loadAnalytics();
  else if (choice !== "denied") showBanner(false);

  document.addEventListener("click", (event) => {
    const trigger = event.target.closest("[data-consent-open]");
    if (!trigger) return;
    opener = trigger;
    // Reopened on purpose, so move focus into the banner.
    showBanner(true);
  });
})();
