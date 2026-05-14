function loadData() {
  const saved = localStorage.getItem("homepageData");
  if (saved) {
    try {
      return JSON.parse(saved);
    } catch {}
  }
  return {
    services: [...(window.DEFAULT_SERVICES || [])],
    addresses: [...(window.DEFAULT_ADDRESSES || [])],
  };
}

function saveData(data) {
  localStorage.setItem("homepageData", JSON.stringify(data));
}

let data = loadData();
let editMode = false;
const iconCache = JSON.parse(localStorage.getItem("iconCache") || "{}");

async function fetchFavicon(url) {
  try {
    const domain = new URL(url).origin;
    if (iconCache[domain] && Date.now() - iconCache[domain].ts < 86400000) {
      return iconCache[domain].icon;
    }
    const faviconUrl = domain + "/favicon.ico";
    await fetch(faviconUrl, { mode: "no-cors" });
    iconCache[domain] = { icon: faviconUrl, ts: Date.now() };
    localStorage.setItem("iconCache", JSON.stringify(iconCache));
    return faviconUrl;
  } catch {
    return null;
  }
}

let dragSrcIdx = null;
let dragSrcType = null;

function createShortcutEl(s, idx, type, clickable) {
  const el = document.createElement("div");
  el.className = "shortcut" + (clickable ? "" : " no-link");
  el.draggable = !editMode;
  el.dataset.idx = idx;
  el.dataset.type = type;

  // Delete button (X)
  const deleteBtn = document.createElement("button");
  deleteBtn.className = "delete-btn";
  deleteBtn.textContent = "×";
  deleteBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    data[type].splice(idx, 1);
    saveData(data);
    render();
  });
  el.appendChild(deleteBtn);

  const iconDiv = document.createElement("div");
  iconDiv.className = "icon";

  if (clickable) {
    fetchFavicon(s.url).then((favicon) => {
      if (favicon) {
        const img = document.createElement("img");
        img.src = favicon;
        img.onerror = () => {
          iconDiv.textContent = s.name[0].toUpperCase();
        };
        iconDiv.appendChild(img);
      } else {
        iconDiv.textContent = s.name[0].toUpperCase();
      }
    });
  } else {
    iconDiv.textContent = s.name[0].toUpperCase();
  }

  const nameDiv = document.createElement("div");
  nameDiv.className = "name";
  nameDiv.textContent = s.name;

  el.appendChild(iconDiv);
  el.appendChild(nameDiv);

  if (!clickable) {
    const urlDiv = document.createElement("div");
    urlDiv.className = "url-text";
    urlDiv.textContent = s.url.replace(/^https?:\/\//, "");
    el.appendChild(urlDiv);
  }

  // Click to navigate
  el.addEventListener("click", () => {
    if (editMode) return;
    if (clickable && !el.classList.contains("dragging")) {
      window.location.href = s.url;
    }
  });

  el.addEventListener("dragstart", (e) => {
    if (editMode) return;
    dragSrcIdx = idx;
    dragSrcType = type;
    el.classList.add("dragging");
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", idx);
  });

  el.addEventListener("dragend", () => {
    el.classList.remove("dragging");
    document
      .querySelectorAll(".drag-over, .drag-left, .drag-right")
      .forEach((x) => {
        x.classList.remove("drag-over", "drag-left", "drag-right");
      });
    dragSrcIdx = null;
    dragSrcType = null;
  });

  el.addEventListener("dragover", (e) => {
    e.preventDefault();
    if (editMode) return;
    if (dragSrcType !== type) return;
    if (dragSrcIdx === idx) return;

    e.dataTransfer.dropEffect = "move";

    document
      .querySelectorAll(".drag-over, .drag-left, .drag-right")
      .forEach((x) => {
        x.classList.remove("drag-over", "drag-left", "drag-right");
      });

    const rect = el.getBoundingClientRect();
    const midX = rect.left + rect.width / 2;

    el.classList.add("drag-over");
    if (e.clientX < midX) {
      el.classList.add("drag-left");
    } else {
      el.classList.add("drag-right");
    }
  });

  el.addEventListener("dragleave", () => {
    el.classList.remove("drag-over", "drag-left", "drag-right");
  });

  el.addEventListener("drop", (e) => {
    e.preventDefault();
    if (editMode) return;
    if (dragSrcType !== type) return;
    if (dragSrcIdx === null || dragSrcIdx === idx) return;

    const rect = el.getBoundingClientRect();
    const midX = rect.left + rect.width / 2;
    const dropBefore = e.clientX < midX;

    const arr = data[type];
    const [item] = arr.splice(dragSrcIdx, 1);

    let targetIdx = idx;
    if (dragSrcIdx < idx) targetIdx--;
    if (!dropBefore) targetIdx++;

    arr.splice(targetIdx, 0, item);
    saveData(data);
    render();
  });

  return el;
}

function render() {
  const servicesEl = document.getElementById("services");
  const addressesEl = document.getElementById("addresses");
  servicesEl.innerHTML = "";
  addressesEl.innerHTML = "";

  document.getElementById("services-section").style.display = data.services
    .length
    ? "block"
    : "none";
  document.getElementById("addresses-section").style.display = data.addresses
    .length
    ? "block"
    : "none";

  data.services.forEach((s, i) =>
    servicesEl.appendChild(createShortcutEl(s, i, "services", true)),
  );
  data.addresses.forEach((s, i) =>
    addressesEl.appendChild(createShortcutEl(s, i, "addresses", false)),
  );
}

// Edit mode toggle
const editBtn = document.getElementById("editBtn");
editBtn.addEventListener("click", () => {
  editMode = !editMode;
  document.body.classList.toggle("edit-mode", editMode);
  editBtn.classList.toggle("active", editMode);
  render();
});

// Modal
const modal = document.getElementById("modal");
const inputName = document.getElementById("inputName");
const inputUrl = document.getElementById("inputUrl");
const inputType = document.getElementById("inputType");

document.getElementById("addBtn").addEventListener("click", () => {
  inputName.value = "";
  inputUrl.value = "";
  inputType.value = "services";
  modal.classList.add("show");
  inputName.focus();
});

document.getElementById("btnCancel").addEventListener("click", () => {
  modal.classList.remove("show");
});

document.getElementById("btnSave").addEventListener("click", () => {
  const name = inputName.value.trim();
  const url = inputUrl.value.trim();
  const type = inputType.value;
  if (name && url) {
    data[type].push({ name, url });
    saveData(data);
    render();
    modal.classList.remove("show");
  }
});

modal.addEventListener("click", (e) => {
  if (e.target === modal) modal.classList.remove("show");
});

render();
