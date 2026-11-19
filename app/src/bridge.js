(() => {
  const call = (channel, args) => window.__TAURI__.core.invoke('ipc', { channel, args });
  const send = (channel) => (...args) => { call(channel, args).catch(() => {}); };
  const invoke = (channel) => (...args) => call(channel, args);
  const on = (channel) => (fn) => {
    window.__TAURI__.webviewWindow.getCurrentWebviewWindow().listen(channel, (event) => fn(event.payload));
  };

  window.petAPI = {
    ready: send('ready'),
    onState: on('state'),
    onShow: on('panel:show'),

    setIgnore: send('pet:ignore'),
    onPointer: on('pet:pointer'),
    onChime: on('pet:chime'),
    dragStart: send('pet:drag-start'),
    dragMove: send('pet:drag-move'),
    dragEnd: send('pet:drag-end'),
    action: send('pet:action'),

    answerAsk: send('ask:answer'),

    setView: send('panel:view'),
    close: send('panel:close'),
    sendChat: invoke('chat:send'),
    reply: invoke('chat:reply'),
    seen: send('thread:seen'),
    dismiss: send('thread:dismiss'),
    clearFinished: send('threads:clear'),
    openTerminal: invoke('thread:terminal'),
    openFolder: send('thread:folder'),
    openClaudeApp: send('claude-app:open'),
    openLink: send('link:open'),
    setConfig: send('config:set'),
    pickFolder: invoke('folder:pick'),
    connect: invoke('connect'),
    disconnect: invoke('disconnect'),
    refreshPets: send('pets:refresh'),
    deletePet: send('pets:delete'),
    hatch: invoke('pets:hatch'),
    importPet: invoke('pets:import'),
    exportPet: invoke('pets:export'),
    clearHistory: send('history:clear'),
    turnOff: send('pet:off'),
  };
})();
