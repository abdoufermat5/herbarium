(function () {
  var AREAS = ["local", "personal", "shared"];
  var MAX_BYTES = 1048576;
  var PERSIST = window.__herbariumPersist !== false;
  var initial = window.__herbariumState;
  if (!initial || typeof initial !== "object") initial = {};
  var maps = {};
  for (var i = 0; i < AREAS.length; i++) {
    var name = AREAS[i];
    var copy = {};
    var src = initial[name];
    if (src && typeof src === "object") {
      for (var k in src) {
        if (Object.prototype.hasOwnProperty.call(src, k) && typeof src[k] === "string") copy[k] = src[k];
      }
    }
    maps[name] = copy;
  }
  var session = {};
  var encoder = typeof TextEncoder === "function" ? new TextEncoder() : null;
  var MAX_KEY_CHARS = 200;

  // Count Unicode code points, matching the core's `chars().count()`: a
  // surrogate pair is one character.
  function keyLength(s) {
    var n = 0;
    for (var i = 0; i < s.length; i++) {
      var c = s.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
        var next = s.charCodeAt(i + 1);
        if (next >= 0xdc00 && next <= 0xdfff) i++;
      }
      n++;
    }
    return n;
  }

  // Core `storage.write` rejects the whole batch when a key is empty or longer
  // than 200 characters. Reject here, before touching memory, so the page gets
  // the error immediately instead of the change being silently dropped.
  function keyError(length) {
    var message =
      length === 0
        ? "The key must be 1 to 200 characters long; empty keys are not allowed."
        : "The key must be 1 to 200 characters long.";
    try { return new DOMException(message, "SyntaxError"); }
    catch (e) { var err = new Error(message); err.name = "SyntaxError"; return err; }
  }

  function keyFailure(key) {
    var length = keyLength(key);
    return length < 1 || length > MAX_KEY_CHARS ? keyError(length) : null;
  }

  function checkKey(key) {
    var err = keyFailure(key);
    if (err) throw err;
  }

  function utf8Length(s) {
    if (encoder) return encoder.encode(s).length;
    var n = 0;
    for (var i = 0; i < s.length; i++) {
      var c = s.charCodeAt(i);
      if (c < 0x80) n += 1;
      else if (c < 0x800) n += 2;
      else if (c >= 0xd800 && c <= 0xdbff) { n += 4; i++; }
      else n += 3;
    }
    return n;
  }

  function sizeOf(state) { return utf8Length(JSON.stringify(state)); }

  function quotaError() {
    var message = "The page's saved data would exceed 1 MiB.";
    try { return new DOMException(message, "QuotaExceededError"); }
    catch (e) { var err = new Error(message); err.name = "QuotaExceededError"; return err; }
  }

  function candidate(areaName, key, value) {
    var next = {};
    for (var i = 0; i < AREAS.length; i++) {
      var name = AREAS[i];
      var from = maps[name];
      var copy = {};
      for (var k in from) if (Object.prototype.hasOwnProperty.call(from, k)) copy[k] = from[k];
      next[name] = copy;
    }
    if (value === null) delete next[areaName][key];
    else next[areaName][key] = String(value);
    return next;
  }

  var pending = [];
  var scheduled = false;

  function schedule() {
    if (!PERSIST || scheduled || pending.length === 0) return;
    scheduled = true;
    Promise.resolve().then(function () {
      scheduled = false;
      if (pending.length === 0) return;
      var changes = pending;
      pending = [];
      try {
        if (window.parent && window.parent !== window) {
          window.parent.postMessage({ type: "herbarium:storage", changes: changes }, "*");
        }
      } catch (e) {}
    });
  }

  function persist(areaName, key, value) {
    var next = candidate(areaName, key, value);
    if (sizeOf(next) > MAX_BYTES) throw quotaError();
    for (var i = 0; i < AREAS.length; i++) maps[AREAS[i]] = next[AREAS[i]];
    pending.push({ area: areaName, key: key, value: value === null ? null : String(value) });
    schedule();
  }

  function read(areaName, key) {
    var m = maps[areaName];
    return Object.prototype.hasOwnProperty.call(m, key) ? m[key] : null;
  }

  function define(target, name, value) {
    try {
      Object.defineProperty(target, name, { value: value, configurable: true, writable: true });
      return;
    } catch (e) {}
    try { target[name] = value; } catch (e2) {}
  }

  function webStorage(areaName, transient) {
    function store() { return transient ? session : maps[areaName]; }
    return {
      getItem: function (key) {
        key = String(key);
        var m = store();
        return Object.prototype.hasOwnProperty.call(m, key) ? m[key] : null;
      },
      setItem: function (key, value) {
        key = String(key);
        value = String(value);
        if (transient) { store()[key] = value; return; }
        checkKey(key);
        persist(areaName, key, value);
      },
      removeItem: function (key) {
        key = String(key);
        if (transient) { delete store()[key]; return; }
        checkKey(key);
        if (Object.prototype.hasOwnProperty.call(maps[areaName], key)) persist(areaName, key, null);
      },
      clear: function () {
        if (transient) {
          var m = store();
          for (var k in m) if (Object.prototype.hasOwnProperty.call(m, k)) delete m[k];
          return;
        }
        var keys = Object.keys(maps[areaName]);
        for (var i = 0; i < keys.length; i++) persist(areaName, keys[i], null);
      },
      key: function (index) {
        var keys = Object.keys(store());
        index = Number(index);
        return index >= 0 && index < keys.length ? keys[index] : null;
      },
      get length() { return Object.keys(store()).length; }
    };
  }

  function artifactArea(shared) { return shared ? "shared" : "personal"; }

  var artifactStorage = {
    get: function (key, shared) {
      key = String(key);
      var bad = keyFailure(key);
      if (bad) return Promise.reject(bad);
      var value = read(artifactArea(shared), key);
      return Promise.resolve(value === null ? null : { key: key, value: value, shared: !!shared });
    },
    set: function (key, value, shared) {
      key = String(key);
      value = String(value);
      var bad = keyFailure(key);
      if (bad) return Promise.reject(bad);
      persist(artifactArea(shared), key, value);
      return Promise.resolve({ key: key, value: value, shared: !!shared });
    },
    delete: function (key, shared) {
      key = String(key);
      var bad = keyFailure(key);
      if (bad) return Promise.reject(bad);
      persist(artifactArea(shared), key, null);
      return Promise.resolve({ key: key, deleted: true, shared: !!shared });
    },
    list: function (prefix, shared) {
      var areaName = artifactArea(shared);
      var p = prefix === undefined || prefix === null ? "" : String(prefix);
      var keys = Object.keys(maps[areaName]).filter(function (k) { return k.indexOf(p) === 0; }).sort();
      return Promise.resolve({ keys: keys, prefix: p, shared: !!shared });
    }
  };

  define(window, "localStorage", webStorage("local", false));
  define(window, "sessionStorage", webStorage("session", true));
  define(window, "storage", artifactStorage);
})();