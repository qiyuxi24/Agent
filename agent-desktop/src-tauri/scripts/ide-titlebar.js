(function() {
  'use strict';

  // ─── 配置 ──────────────────────────────────────────
  var BRIDGE_PORT = 19527;
  // 可在 code-server 启动前通过 window.__VOTEK_BRIDGE_CONFIG__ 覆盖
  var BRIDGE_HTTP = 'http://127.0.0.1:' + BRIDGE_PORT + '/';

  // ─── 注入 CSS 样式 ──────────────────────────────
  var style = document.createElement('style');
  style.textContent = [
    '#votek-titlebar{display:flex;align-items:center;justify-content:space-between;',
    'height:32px;min-height:32px;padding:0 10px;',
    'background:rgba(30,30,30,0.95);border-bottom:1px solid rgba(255,255,255,0.08);',
    'user-select:none;flex-shrink:0;position:fixed;top:0;left:0;right:0;z-index:999999;}',
    '#votek-titlebar-left{display:flex;align-items:center;gap:8px;}',
    '#votek-titlebar-title{font-size:12px;font-weight:600;color:rgba(255,255,255,0.5);',
    'letter-spacing:0.5px;pointer-events:none;}',
    '#votek-bridge-status{display:inline-flex;align-items:center;gap:4px;',
    'font-size:10px;padding:2px 6px;border-radius:8px;opacity:0.7;transition:all 0.3s;}',
    '#votek-bridge-status .dot{width:6px;height:6px;border-radius:50%;display:inline-block;}',
    '#votek-bridge-status.connected{color:rgba(80,220,120,0.8);}',
    '#votek-bridge-status.connected .dot{background:rgba(80,220,120,0.9);box-shadow:0 0 4px rgba(80,220,120,0.4);}',
    '#votek-bridge-status.disconnected{color:rgba(255,255,255,0.25);}',
    '#votek-bridge-status.disconnected .dot{background:rgba(255,255,255,0.2);}',
    '#votek-bridge-status.hidden{display:none;}',
    '#votek-titlebar-controls{display:flex;align-items:center;gap:2px;}',
    '.votek-titlebar-btn{display:flex;align-items:center;justify-content:center;',
    'width:36px;height:28px;border:none;background:transparent;color:rgba(255,255,255,0.5);',
    'cursor:pointer;border-radius:0;font-size:14px;transition:all 0.08s;font-family:inherit;}',
    '.votek-titlebar-btn:hover{background:rgba(255,255,255,0.10);color:rgba(255,255,255,0.8);}',
    '.votek-titlebar-btn-close:hover{background:#e81123;color:white;}',
    'body{padding-top:32px!important;}'
  ].join('');
  document.head.appendChild(style);

  // ─── 创建标题栏 DOM ────────────────────────────
  var tb = document.createElement('div');
  tb.id = 'votek-titlebar';
  tb.setAttribute('data-tauri-drag-region', '');
  tb.innerHTML =
    '<div id="votek-titlebar-left">' +
      '<span id="votek-titlebar-title">Votek — IDE</span>' +
      '<span id="votek-bridge-status" class="hidden">' +
        '<span class="dot"></span>Bridge' +
      '</span>' +
    '</div>' +
    '<div id="votek-titlebar-controls">' +
      '<button class="votek-titlebar-btn" id="votek-btn-minimize" title="最小化">&minus;</button>' +
      '<button class="votek-titlebar-btn" id="votek-btn-maximize" title="最大化">&#x25A1;</button>' +
      '<button class="votek-titlebar-btn votek-titlebar-btn-close" id="votek-btn-close" title="关闭">&times;</button>' +
    '</div>';
  document.body.prepend(tb);

  // ─── 桥接状态检测（通过 HTTP 健康检查） ─────────
  var bridgeStatus = document.getElementById('votek-bridge-status');

  function checkBridge() {
    fetch(BRIDGE_HTTP, { method: 'GET', mode: 'no-cors', cache: 'no-store' })
      .then(function(resp) {
        if (resp.ok || resp.type === 'opaque') {
          bridgeStatus.className = 'connected';
          bridgeStatus.title = 'Votek Bridge 已连接 (端口 ' + BRIDGE_PORT + ')';
        } else {
          throw new Error('status ' + resp.status);
        }
      })
      .catch(function() {
        bridgeStatus.className = 'disconnected';
        bridgeStatus.title = 'Votek Bridge 未连接';
      });
    bridgeStatus.classList.remove('hidden');
  }

  // 延迟检查：等 Companion 扩展完成初始化
  setTimeout(checkBridge, 3000);
  // 每 30 秒刷新一次
  setInterval(checkBridge, 30000);

  // ─── 绑定 Tauri 窗口控制 ────────────────────────
  function updateMaximizeBtn(mBtn, inv) {
    inv.invoke('plugin:window|is-maximized').then(function(isMax) {
      mBtn.innerHTML = isMax ? '&#x29C9;' : '&#x25A1;';
      mBtn.title = isMax ? '还原' : '最大化';
    });
  }

  function bindControls() {
    var inv = window.__TAURI_INTERNALS__;
    if (!inv) { setTimeout(bindControls, 100); return; }

    // 最小化
    document.getElementById('votek-btn-minimize').addEventListener('click', function(e) {
      e.stopPropagation();
      inv.invoke('plugin:window|minimize');
    });

    // 最大化/还原
    var mBtn = document.getElementById('votek-btn-maximize');
    mBtn.addEventListener('click', function(e) {
      e.stopPropagation();
      inv.invoke('plugin:window|toggle-maximize').then(function() {
        updateMaximizeBtn(mBtn, inv);
      });
    });

    // 关闭
    document.getElementById('votek-btn-close').addEventListener('click', function(e) {
      e.stopPropagation();
      inv.invoke('plugin:window|close');
    });

    // 初始化按钮状态
    updateMaximizeBtn(mBtn, inv);

    // 窗口大小变化时更新按钮状态
    window.addEventListener('resize', function() {
      updateMaximizeBtn(mBtn, inv);
    });
  }
  bindControls();

  // ─── 修正页面标题（VS Code 动态加载后可能覆盖 title）──
  var attempts = 0;
  var fixTitle = function() {
    if (document.title && document.title.indexOf('Votek') === -1 && document.title.indexOf('code-server') !== -1) {
      document.title = document.title.replace(/code-server/gi, 'Votek');
    }
    if (document.title && document.title.startsWith('Code - ')) {
      document.title = document.title.replace('Code - ', '');
    }
    if (++attempts < 30) setTimeout(fixTitle, 1000);
  };
  setTimeout(fixTitle, 500);
})();
