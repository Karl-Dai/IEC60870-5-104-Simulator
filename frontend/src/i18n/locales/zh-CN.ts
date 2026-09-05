export type DictShape = {
  common: {
    confirm: string
    cancel: string
    ok: string
    close: string
    save: string
    refresh: string
    clear: string
    export: string
    delete: string
    add: string
    loading: string
    browse: string
    certificateFiles: string
    privateKeyFiles: string
  }
  toolbar: {
    menuConfig: string
    menuNew: string
    menuPoints: string
    menuSettings: string
    menuTools: string
    menuHelp: string
    currentScope: string
    currentServer: string
    allServers: string
    newServer: string
    start: string
    startAll: string
    titleStartAll: string
    startAllProgress: string
    startAllEmpty: string
    startAllResult: string
    startAllFailed: string
    stopAll: string
    titleStopAll: string
    stopAllProgress: string
    stopAllEmpty: string
    stopAllResult: string
    stopAllFailed: string
    stop: string
    addStation: string
    randomMutation: string
    stopMutation: string
    cyclicSend: string
    stopCyclic: string
    mutationInterval: string
    sendInterval: string
    appTitle: string
    about: string
    titleNewServer: string
    titleStartServer: string
    titleStopServer: string
    titleAddStation: string
    titleRandomMutation: string
    titleCyclicSend: string
    checkUpdate: string
    checkingUpdate: string
    alreadyLatest: string
    updateCheckFailed: string
    updateCheckFailedMirrorPrompt: string
    parseFrame: string
    parseFrameInLog: string
    saveConfig: string
    openConfig: string
    openConfigByPath: string
    openConfigByPathTitle: string
    configPathPrompt: string
    configPathRequired: string
    configSaved: string
    configLoaded: string
    configSaveFailed: string
    configLoadFailed: string
    importCsv: string
    exportCsv: string
    downloadCsvTemplate: string
    csvExported: string
    csvTemplateSaved: string
    csvImported: string
    csvExportFailed: string
    csvTemplateFailed: string
    csvImportFailed: string
    csvImportErrorHint: string
    csvImportStoppedOnly: string
    csvImportModeTitle: string
    csvImportModeHint: string
    csvMerge: string
    csvMergeHint: string
    csvReplace: string
    csvReplaceHint: string
    csvReplaceConfirm: string
  }
  newServer: {
    title: string
    createAndStart: string
    creating: string
    retry: string
    failureKept: string
    bindAddressLabel: string
    bindAddressHint: string
    portLabel: string
    commonAddressLabel: string
    stationNameLabel: string
    stationNamePlaceholder: string
    initMode: string
    initZero: string
    initRandom: string
    countPerCategory: string
    countHint: string
    enableTls: string
    serverCert: string
    serverKey: string
    caFile: string
    requireClientCert: string
  }
  serverSettings: {
    title: string
    entry: string
    loading: string
    runningHint: string
    stopAndEdit: string
    stopping: string
    tlsOnHint: string
    tlsOffHint: string
    saveHint: string
    requiredFiles: string
    caRequiredLabel: string
    reload: string
  }
  prompt: {
    inputCommonAddress: string
    inputStationName: string
    defaultStationName: string
  }
  station: {
    defaultName: string
    unnamedName: string
    displayName: string
  }
  tree: {
    title: string
    noServers: string
    noServersHint: string
    ctxStartServer: string
    ctxStopServer: string
    ctxDeleteServer: string
    ctxDeleteStation: string
    ctxEditStation: string
    ctxEditRuntimeParams: string
    ctxViewConnections: string
    connTooltip: string
    confirmDeleteServer: string
    confirmDeleteRunningServer: string
    confirmDeleteStation: string
  }
  runtimeParams: {
    title: string
    save: string
    cancel: string
    saving: string
    loading: string
  }
  connections: {
    title: string
    summary: string
    empty: string
    emptyHint: string
    stateActive: string
    stateConnected: string
    colPeer: string
    colState: string
  }
  category: {
    single_point: string
    double_point: string
    step_position: string
    bitstring: string
    normalized_measured: string
    scaled_measured: string
    float_measured: string
    integrated_totals: string
    single_command: string
    double_command: string
    step_command: string
    bitstring_command: string
    normalized_setpoint: string
    scaled_setpoint: string
    float_setpoint: string
  }
  asduType: {
    sp: string
    sp_ta: string
    sp_tb: string
    dp: string
    dp_ta: string
    dp_tb: string
    st: string
    st_ta: string
    st_tb: string
    bo: string
    bo_tb: string
    me_na: string
    me_ta: string
    me_td: string
    me_nd: string
    me_nb: string
    me_tb: string
    me_te: string
    me_nc: string
    me_tc: string
    me_tf: string
    it: string
    it_tb: string
    c_sc_na: string
    c_dc_na: string
    c_rc_na: string
    c_se_na: string
    c_se_nb: string
    c_se_nc: string
    c_bo_na: string
    c_sc_ta: string
    c_dc_ta: string
    c_rc_ta: string
    c_se_ta: string
    c_se_tb: string
    c_se_tc: string
    c_bo_ta: string
  }
  table: {
    allPoints: string
    countSuffix: string
    searchPlaceholder: string
    addPointTitle: string
    batchAdd: string
    batchWrite: string
    chooseStation: string
    chooseStationHint: string
    noPoints: string
    noPointsHint: string
    asduTypeCol: string
    nameCol: string
    valueCol: string
    qualityCol: string
    timestampCol: string
    deletePoint: string
    editPoint: string
    startMutation: string
    stopMutation: string
    mutationPeriod: string
    mutationMode: string
    modeFlip: string
    modeIncrement: string
    modeDecrement: string
    modeRandom: string
    mutationStep: string
    mutationMin: string
    mutationMax: string
    dpIntermediate: string
    dpIndeterminate: string
    derivedTbTitle: string
    dupIoaTitle: string
    batchControlOptions: string
    batchTypeMigration: string
    batchSettings: string
    selectFiltered: string
    invertFiltered: string
    clearSelection: string
    selectedCount: string
    enterMultiSelect: string
    exitMultiSelect: string
    sortAscending: string
    sortDescending: string
    controlIntentDirection: string
    controlIntentTitle: string
    controlIntentBody: string
  }
  simulationSettings: {
    open: string
    title: string
    selectionHint: string
    noSelection: string
    mixedValues: string
    activeTitle: string
    processing: string
    previousPage: string
    nextPage: string
    pageCount: string
    noActive: string
    currentValue: string
    apply: string
    stopSelected: string
    stop: string
    periodRange: string
    stepInvalid: string
    boundsInvalid: string
  }
  pointModal: {
    title: string
    editTitle: string
    ioaLabel: string
    ioaPlaceholder: string
    ioaEditHint: string
    dupIoaWarn: string
    dupIoaSameTypeError: string
    asduTypeLabel: string
    nameLabel: string
    namePlaceholder: string
    commentLabel: string
    commentPlaceholder: string
    saving: string
    add: string
    save: string
    mappingLabel: string
    mappingNone: string
    mappingHint: string
    qualifierLabel: string
    qualifierHint: string
    quAny: string
    qu0: string
    qu1: string
    qu2: string
    qu3: string
    ql0: string
    quCustom: string
    executionModeLabel: string
    executionModeFlexible: string
    executionModeDirect: string
    executionModeSbo: string
  }
  batchModal: {
    title: string
    startIoa: string
    startIoaAutoHint: string
    count: string
    asduTypeLabel: string
    namePrefix: string
    namePrefixPlaceholder: string
    nameWithTypeId: string
    namePatternExample: string
    countWarn: string
    rangeHint: string
    modeRange: string
    modeExpression: string
    expressionLabel: string
    expressionPlaceholder: string
    expressionError: string
    expressionHint: string
    existingSameType: string
    conflictWarn: string
    saving: string
    add: string
    failedPrefix: string
    nextIoaBtn: string
    nextGapBtn: string
    capacityFullTooltip: string
    conflictDetail: string
    crossTypeDup: string
  }
  batchControl: {
    title: string
    selectionHint: string
    applyQualifier: string
    applySbo: string
    apply: string
    appliedResult: string
    qualifierRange: string
  }
  batchType: {
    title: string
    selectionHint: string
    targetType: string
    preserveHint: string
    changedCount: string
    apply: string
    appliedResult: string
    failed: string
  }
  batchWrite: {
    title: string
    typeLabel: string
    ioaLabel: string
    ioaPlaceholder: string
    valueLabel: string
    hit: string
    ignored: string
    ignoredDetail: string
    parseError: string
    write: string
    writeN: string
    writing: string
    failedPrefix: string
    phSingle: string
    phDouble: string
    phStep: string
    phBitstring: string
    phNormalized: string
    phScaled: string
    phFloat: string
    phTotal: string
  }
  valuePanel: {
    title: string
    selectPointHint: string
    selectPointHintSub: string
    sectionInfo: string
    asduType: string
    category: string
    name: string
    comment: string
    mapping: string
    qualifierQoc: string
    qualifierQl: string
    qualifierAny: string
    qoc0: string
    qoc1: string
    qoc2: string
    qoc3: string
    qocReserved: string
    ql0: string
    qlOther: string
    executionMode: string
    executionFlexible: string
    executionDirect: string
    executionSbo: string
    sectionCurrent: string
    value: string
    quality: string
    qualityNa: string
    qualityValid: string
    qualityInvalid: string
    timestamp: string
    sectionWrite: string
    valuePlaceholder: string
    write: string
    sectionMultiSelect: string
    countLabel: string
    applyQuality: string
    applyValue: string
    batchValueMixedHint: string
  }
  quality: {
    legendTitle: string
    bits: Record<'iv' | 'nt' | 'sb' | 'bl' | 'ov', { name: string; desc: string }>
  }
  doublePoint: {
    legendTitle: string
    tokens: Record<'intermediate' | 'off' | 'on' | 'indeterminate', string>
    states: Record<'intermediate' | 'off' | 'on' | 'indeterminate', string>
  }
  log: {
    title: string
    refresh: string
    clear: string
    export: string
    exporting: string
    exportFailed: string
    loading: string
    chooseServer: string
    noLogs: string
    noMatches: string
    autoScroll: string
    allDirections: string
    allFrames: string
    iFrames: string
    sFrames: string
    uFrames: string
    searchPlaceholder: string
    filteredCount: string
    resizeColumn: string
    timeCol: string
    directionCol: string
    frameCol: string
    detailCol: string
    rawCol: string
    titleRefresh: string
    titleClear: string
    titleExport: string
    backendDetailFallback: string
    serverStarted: string
    serverStopped: string
    cmdRejected: string
    cmdCancel: string
    cmdSelect: string
    cmdExecute: string
    cmdMappingBroken: string
    cmdMalformed: string
    unknownType: string
    /** 后端 detail_event 键:时钟同步应答开关关闭导致的拒收(payload: ca)。 */
    clockSyncDisabled: string
    clockSyncInvalidCot: string
    clockSyncInvalidIoa: string
    clockSyncMalformed: string
  }
  about: {
    whatsNew: string
    homepage: string
    onlineDemoLabel: string
    homepageLabel: string
    releasesLabel: string
    copiedSuffix: string
  }
  appDialog: {
    cancel: string
    ok: string
    titleAlert: string
    titleConfirm: string
    titlePrompt: string
    backendMessageFallback: string
  }
  errors: {
    invalidPort: string
    invalidCa: string
    stationCaRunning: string
    invalidIoa: string
    startBindInUse: string
    startBindDenied: string
    startBindUnavailable: string
    startFailed: string
  }
  update: {
    available: string
    newVersion: string
    changelog: string
    ready: string
    installNow: string
    installNextLaunch: string
    skip: string
    working: string
    failedTitle: string
  }
  parseFrame: {
    title: string
    hint: string
    hexLabel: string
    templatesLabel: string
    errEmpty: string
    parse: string
    parsing: string
    apciI: string
    apciS: string
    apciU: string
    bytes: string
    startByte: string
    apduLength: string
    controlField: string
    seqNo: string
    typeRow: string
    cotNegative: string
    cotTest: string
    cotNames: Record<string, string>
    oa: string
    ca: string
    objects: string
    objectsCount: string
    colValue: string
    colQuality: string
    colTimestamp: string
    colRaw: string
    dpIntermediate: string
    dpIndeterminate: string
  }
  remoteParams: {
    linkParams: string
    linkParamsSub: string
    hintT0: string
    hintT1: string
    hintT2: string
    hintT3: string
    hintK: string
    hintW: string
    autoCorrected: string
    interrogation: string
    interrogationSub: string
    answerSwitches: string
    gi: string
    counterInterrogation: string
    commands: string
    clockSync: string
    clockSyncHint: string
    sendActTerm: string
    sendActTermHint: string
    executeCotDisabledHint: string
    appLayerNote: string
    controlMappingHint: string
    autoMapCommands: string
    ackUnmappedCommands: string
    sboEnforce: string
    sboTimeout: string
    giWithTimestamp: string
    cmdAckCot: string
    select: string
    execute: string
    cancel: string
    uploadMode: string
    uploadModeSub: string
    sqMode: string
    untimestamped: string
    timestamped: string
    packingStrategy: string
    autoPacking: string
    syncTb: string
    syncTbNote: string
    mutationSim: string
    pacingHint: string
    pacingSaved: string
    randomPacing: string
    perSend: string
    unitCount: string
    delay: string
    modeContinuous: string
    modeDiscrete: string
    connParams: string
    connParamsSub: string
    bindAddress: string
    port: string
    runningHint: string
    stopBeforeEdit: string
    drawerTitle: string
    discard: string
    discardTitle: string
    closeEsc: string
    loadingText: string
    footNote: string
    selectServerFirst: string
    saving: string
    saved: string
    saveAll: string
    configTimingCorrected: string
  }
  _test: { interp: string }
}

const dict: DictShape = {
  common: {
    confirm: '确认',
    cancel: '取消',
    ok: '确定',
    close: '关闭',
    save: '保存',
    refresh: '刷新',
    clear: '清空',
    export: '导出',
    delete: '删除',
    add: '添加',
    loading: '加载中...',
    browse: '浏览…',
    certificateFiles: '证书文件',
    privateKeyFiles: '私钥文件',
  },
  toolbar: {
    menuConfig: '配置',
    menuNew: '新建',
    menuPoints: '点表',
    menuSettings: '设置',
    menuTools: '工具',
    menuHelp: '帮助',
    currentScope: '当前',
    currentServer: '当前服务器',
    allServers: '全部服务器',
    newServer: '新建服务器',
    start: '启动',
    startAll: '全部启动',
    titleStartAll: '启动所有未运行的服务器及其下属站，跳过已运行项',
    startAllProgress: '启动中 {completed}/{total}',
    startAllEmpty: '暂无服务器，请先新建服务器或加载配置。',
    startAllResult: '全部启动完成：成功 {started} 个，已运行跳过 {skipped} 个，失败 {failed} 个。',
    startAllFailed: '无法获取服务器列表',
    stopAll: '全部停止',
    titleStopAll: '停止所有运行中的服务器及其下属站，断开连接并释放监听端口，保留配置',
    stopAllProgress: '停止中 {completed}/{total}',
    stopAllEmpty: '暂无服务器，无需停止。',
    stopAllResult: '全部停止完成：成功 {stopped} 个，已停止跳过 {skipped} 个，失败 {failed} 个。',
    stopAllFailed: '无法获取服务器列表',
    stop: '停止',
    addStation: '添加站',
    randomMutation: '随机变化',
    stopMutation: '停止变化',
    cyclicSend: '周期发送',
    stopCyclic: '停止周期',
    mutationInterval: '变化间隔 (ms)',
    sendInterval: '发送间隔 (ms)',
    appTitle: 'IEC104 Slave',
    about: '关于',
    titleNewServer: '新建服务器',
    titleStartServer: '启动服务器',
    titleStopServer: '停止服务器',
    titleAddStation: '添加站',
    titleRandomMutation: '随机变化',
    titleCyclicSend: '周期发送',
    checkUpdate: '检查更新',
    checkingUpdate: '检查中…',
    alreadyLatest: '已是最新版本',
    updateCheckFailed: '更新检查失败',
    updateCheckFailedMirrorPrompt: '更新检查失败,可能无法访问 GitHub。是否打开国内镜像下载页面?',
    parseFrame: '报文解析',
    parseFrameInLog: '解析此报文',
    saveConfig: '保存配置',
    openConfig: '打开配置',
    openConfigByPath: '输入路径',
    openConfigByPathTitle: '通过文件路径打开配置',
    configPathPrompt: '粘贴 JSON 配置文件的完整路径。建议使用已下载到本地的文件。',
    configPathRequired: '请输入配置文件的完整路径',
    configSaved: '配置已保存',
    configLoaded: '已加载 {count} 个服务器',
    configSaveFailed: '保存失败',
    configLoadFailed: '打开失败',
    importCsv: '导入 CSV',
    exportCsv: '导出 CSV',
    downloadCsvTemplate: '下载模板',
    csvExported: '已导出 {count} 个点位',
    csvTemplateSaved: 'CSV 模板已保存',
    csvImported: '已导入 {count} 个点位，当前站共 {total} 个点位，恢复 {mutations} 个模拟任务',
    csvExportFailed: '导出 CSV 失败',
    csvTemplateFailed: '保存 CSV 模板失败',
    csvImportFailed: '导入 CSV 失败',
    csvImportErrorHint: '本次导入未写入任何点位。下方保留完整错误明细；校验错误包含对应行号和原因，可滚动查看并复制修正。',
    csvImportStoppedOnly: 'CSV 导入会替换点位定义，请先停止当前服务器再导入',
    csvImportModeTitle: '选择 CSV 导入方式',
    csvImportModeHint: 'CSV 将导入当前选中的站点。所有行会先完整校验；任何错误都会取消整次导入。',
    csvMerge: '合并',
    csvMergeHint: '保留当前点位，只添加 CSV 中的新点位；遇到重复点位会整批拒绝。',
    csvReplace: '替换',
    csvReplaceHint: '清空当前站点的点位和模拟任务，再按 CSV 完整重建。',
    csvReplaceConfirm: '替换会清空当前站点的全部点位和模拟任务，然后按 CSV 重建。确定继续吗？',
  },
  newServer: {
    title: '新建服务器',
    createAndStart: '创建并启动',
    creating: '正在创建…',
    retry: '重试创建',
    failureKept: '创建或启动失败，填写内容已保留。',
    bindAddressLabel: '监听地址',
    bindAddressHint: '0.0.0.0 监听全部网卡；也可输入指定网卡的 IP 地址。',
    portLabel: '端口号',
    commonAddressLabel: 'ASDU 公共地址 (CA)',
    stationNameLabel: '站名',
    stationNamePlaceholder: '可选，例如：220kV 站',
    initMode: '初始值',
    initZero: '全零',
    initRandom: '随机',
    countPerCategory: '每类点数',
    countHint: '0 = 空配置（推荐）。同一 CASDU 内 IOA 应唯一，建议按需批量创建点位。',
    enableTls: '启用 TLS',
    serverCert: '服务器证书文件 (PEM)',
    serverKey: '服务器密钥文件 (PEM)',
    caFile: 'CA 证书文件 (PEM, 可选)',
    requireClientCert: '要求客户端证书 (mTLS)',
  },
  serverSettings: {
    title: '服务器设置',
    entry: '服务器设置（地址 / TLS）',
    loading: '正在读取服务器设置…',
    runningHint: '服务器正在运行。修改地址或 TLS 前需先停止，这会断开当前客户端连接。',
    stopAndEdit: '停止并编辑',
    stopping: '正在停止…',
    tlsOnHint: '使用 TLS 加密通信；启用双向认证时必须填写 CA 证书。',
    tlsOffHint: '关闭 TLS 后使用普通 TCP，原证书路径会保留，方便下次启用。',
    saveHint: '保存后保持停止状态',
    requiredFiles: '请填写服务器证书和私钥；启用双向认证时还需填写 CA 证书。',
    caRequiredLabel: 'CA 证书文件 (PEM, 必填)',
    reload: '重新读取',
  },
  prompt: {
    inputCommonAddress: '输入公共地址 (CA)',
    inputStationName: '输入站名',
    defaultStationName: '站 {ca}',
  },
  station: {
    defaultName: '站',
    unnamedName: '站',
    displayName: '{name}（CA:{ca}）',
  },
  tree: {
    title: '服务器',
    noServers: '暂无服务器',
    noServersHint: '从「新建」菜单选择「新建服务器」开始',
    ctxStartServer: '启动服务器',
    ctxStopServer: '停止服务器',
    ctxDeleteServer: '删除服务器',
    ctxDeleteStation: '删除站',
    ctxEditStation: '编辑站配置',
    ctxEditRuntimeParams: '修改运行参数',
    ctxViewConnections: '查看主站连接（{n}）',
    connTooltip: '已连接 {n} 个主站',
    confirmDeleteServer: '确定删除服务器 {server}？未保存的点表数据将丢失（可先「保存配置」）。',
    confirmDeleteRunningServer: '服务器 {server} 正在运行！删除会先停止监听并断开全部客户端，未保存的点表数据将丢失（可先「保存配置」）。确定删除？',
    confirmDeleteStation: '确定删除站 CA={ca} 及其全部数据点？',
  },
  runtimeParams: {
    title: '修改运行参数',
    save: '保存',
    cancel: '取消',
    saving: '保存中...',
    loading: '加载中...',
  },
  connections: {
    title: '连接状态',
    summary: '已连接 {n} 个主站',
    empty: '暂无主站连接',
    emptyHint: '服务器运行后，主站连接将显示在此处',
    stateActive: '数据传输中',
    stateConnected: '已连接',
    colPeer: '主站地址',
    colState: '状态',
  },
  category: {
    single_point: '单点 (SP)',
    double_point: '双点 (DP)',
    step_position: '步位置 (ST)',
    bitstring: '位串 (BO)',
    normalized_measured: '归一化 (ME_NA)',
    scaled_measured: '标度化 (ME_NB)',
    float_measured: '浮点 (ME_NC)',
    integrated_totals: '累计量 (IT)',
    single_command: '单点命令 (C_SC)',
    double_command: '双点命令 (C_DC)',
    step_command: '步调节命令 (C_RC)',
    bitstring_command: '位串命令 (C_BO)',
    normalized_setpoint: '归一化设定值 (C_SE_NA)',
    scaled_setpoint: '标度化设定值 (C_SE_NB)',
    float_setpoint: '浮点设定值 (C_SE_NC)',
  },
  asduType: {
    sp: 'M_SP_NA_1 - 单点信息',
    sp_ta: 'M_SP_TA_1 - 单点 (CP24 短时标)',
    sp_tb: 'M_SP_TB_1 - 单点 (带时标)',
    dp: 'M_DP_NA_1 - 双点信息',
    dp_ta: 'M_DP_TA_1 - 双点 (CP24 短时标)',
    dp_tb: 'M_DP_TB_1 - 双点 (带时标)',
    st: 'M_ST_NA_1 - 步位置信息',
    st_ta: 'M_ST_TA_1 - 步位置 (CP24 短时标)',
    st_tb: 'M_ST_TB_1 - 步位置 (带时标)',
    bo: 'M_BO_NA_1 - 位串',
    bo_tb: 'M_BO_TB_1 - 位串 (带时标)',
    me_na: 'M_ME_NA_1 - 归一化测量值',
    me_ta: 'M_ME_TA_1 - 归一化 (CP24 短时标)',
    me_td: 'M_ME_TD_1 - 归一化 (带时标)',
    me_nd: 'M_ME_ND_1 - 归一化 (无品质)',
    me_nb: 'M_ME_NB_1 - 标度化测量值',
    me_tb: 'M_ME_TB_1 - 标度化 (CP24 短时标)',
    me_te: 'M_ME_TE_1 - 标度化 (带时标)',
    me_nc: 'M_ME_NC_1 - 浮点测量值',
    me_tc: 'M_ME_TC_1 - 浮点 (CP24 短时标)',
    me_tf: 'M_ME_TF_1 - 浮点 (带时标)',
    it: 'M_IT_NA_1 - 累计量',
    it_tb: 'M_IT_TB_1 - 累计量 (带时标)',
    c_sc_na: 'C_SC_NA_1 - 单点命令',
    c_dc_na: 'C_DC_NA_1 - 双点命令',
    c_rc_na: 'C_RC_NA_1 - 步调节命令',
    c_se_na: 'C_SE_NA_1 - 归一化设定值',
    c_se_nb: 'C_SE_NB_1 - 标度化设定值',
    c_se_nc: 'C_SE_NC_1 - 浮点设定值',
    c_bo_na: 'C_BO_NA_1 - 位串命令',
    c_sc_ta: 'C_SC_TA_1 - 单点命令 (带时标)',
    c_dc_ta: 'C_DC_TA_1 - 双点命令 (带时标)',
    c_rc_ta: 'C_RC_TA_1 - 步调节命令 (带时标)',
    c_se_ta: 'C_SE_TA_1 - 归一化设定值 (带时标)',
    c_se_tb: 'C_SE_TB_1 - 标度化设定值 (带时标)',
    c_se_tc: 'C_SE_TC_1 - 浮点设定值 (带时标)',
    c_bo_ta: 'C_BO_TA_1 - 位串命令 (带时标)',
  },
  table: {
    allPoints: '全部数据点',
    countSuffix: '个数据点',
    searchPlaceholder: '搜索 IOA / 名称...',
    addPointTitle: '添加数据点',
    batchAdd: '批量添加点位',
    batchWrite: '设置值',
    chooseStation: '选择一个站',
    chooseStationHint: '在左侧导航树中点击一个站查看数据点',
    noPoints: '该站暂无数据点',
    noPointsHint: '用右上角「+」或「批量」添加数据点',
    asduTypeCol: 'ASDU 类型',
    nameCol: '名称',
    valueCol: '值',
    qualityCol: '品质',
    timestampCol: '时间戳',
    deletePoint: '删除数据点',
    editPoint: '编辑点位配置',
    startMutation: '启动周期变位',
    stopMutation: '停止周期变位',
    mutationPeriod: '周期',
    mutationMode: '方式',
    modeFlip: '翻转',
    modeIncrement: '递增',
    modeDecrement: '递减',
    modeRandom: '随机',
    mutationStep: '步长',
    mutationMin: '下限',
    mutationMax: '上限',
    dpIntermediate: '中间',
    dpIndeterminate: '不确定',
    derivedTbTitle: '变位时将追加派生帧 {tb}（「变位同步上送 TB」已开启；点位自身 Type ID 不变）',
    dupIoaTitle: 'IOA {ioa} 与本站其他类型共用：{types}。同一 CASDU 内 IOA 应唯一，请编辑该点修改 IOA。',
    batchControlOptions: '批量设置控制参数',
    batchTypeMigration: '批量修改 ASDU 类型',
    batchSettings: '批量设置',
    selectFiltered: '全选当前筛选',
    invertFiltered: '反选当前筛选',
    clearSelection: '清空选择',
    selectedCount: '已选 {count}',
    enterMultiSelect: '多选',
    exitMultiSelect: '退出多选',
    sortAscending: '升序排列',
    sortDescending: '降序排列',
    controlIntentDirection: '主站 → 子站',
    controlIntentTitle: '为什么子站显示控制点？',
    controlIntentBody: '控制点用于定义主站可下发的命令入口，以及 IOA 映射、QOC/QL 和选择—执行（SBO）策略；它们不参与总召、周期发送或自发上送。',
  },
  simulationSettings: {
    open: '模拟设置',
    title: '点位模拟设置',
    selectionHint: '已选择 {count} 个点位',
    noSelection: '先在点表中选择一个或多个点位；下方仍可查看和停止活动模拟。',
    mixedValues: '所选点位的活动参数不一致；应用后将使用下方参数统一更新。',
    activeTitle: '活动模拟',
    processing: '正在处理 {done} / {total} 个点位，可关闭此面板，操作会继续执行。',
    previousPage: '上一页',
    nextPage: '下一页',
    pageCount: '第 {page} / {total} 页',
    noActive: '当前站没有活动模拟。',
    currentValue: '当前值',
    apply: '启动 / 更新',
    stopSelected: '停止选中点',
    stop: '停止',
    periodRange: '周期必须在 50–60000 ms 之间。',
    stepInvalid: '步长必须是非零数值。',
    boundsInvalid: '下限必须小于或等于上限。',
  },
  pointModal: {
    title: '添加数据点',
    editTitle: '编辑数据点',
    ioaLabel: 'IOA (信息对象地址)',
    ioaPlaceholder: '例如: 100',
    ioaEditHint: '可修改 IOA 改址；运行值与品质保留，引用本点的控制映射会同步更新。',
    dupIoaWarn: '该 IOA 已被本站 {types} 使用。同一 CASDU 内 IOA 应唯一；仍可保存。',
    dupIoaSameTypeError: '本站该 IOA 下已存在同类型点位（{type}），重复添加会覆盖它的名称/备注/限定词/映射，已阻止。请改用编辑该点，或换一个 IOA。',
    asduTypeLabel: 'ASDU 类型',
    nameLabel: '名称 (可选)',
    namePlaceholder: '可留空',
    commentLabel: '备注 (可选)',
    commentPlaceholder: '可留空',
    saving: '添加中...',
    add: '确认',
    save: '保存',
    mappingLabel: '映射到监视点',
    mappingNone: '不映射（仅应答命令）',
    mappingHint: '控制与监视方向独立编址；可跨 CA、跨 IOA 映射到同值族的 NA/TA/TB 点。',
    qualifierLabel: 'QOC / QL 限定词',
    qualifierHint: '控制命令 QU：0..31；设点 QL：0..127。「不限制」表示接受任意值。',
    quAny: '不限制（接受任意值）',
    qu0: '0 = 无附加定义（默认）',
    qu1: '1 = 短脉冲',
    qu2: '2 = 长脉冲',
    qu3: '3 = 持续输出',
    ql0: '0 = 默认',
    quCustom: '自定义',
    executionModeLabel: 'S/E 执行模式',
    executionModeFlexible: '宽松（兼容旧配置）',
    executionModeDirect: '仅执行（直接控制）',
    executionModeSbo: '选择后执行（SBO）',
  },
  batchModal: {
    title: '批量添加数据点',
    startIoa: '起始 IOA',
    startIoaAutoHint: '起始 IOA 已按本站已占用地址自动避让（可手动修改）。',
    count: '数量',
    asduTypeLabel: 'ASDU 类型',
    namePrefix: '名称前缀（可选）',
    namePrefixPlaceholder: '如 SP → SP_0, SP_1, ...',
    nameWithTypeId: '名称包含 Type ID（前缀_类型_IOA）',
    namePatternExample: '名称示例：{example}',
    countWarn: '范围过大（最多 100000）',
    rangeHint: 'IOA 范围：{startIoa} ~ {endIoa}，共将添加 {count} 个数据点',
    modeRange: '连续范围',
    modeExpression: '表达式',
    expressionLabel: 'IOA 表达式',
    expressionPlaceholder: '如 6001-6050 或 6001, 6003, 6006, 6012',
    expressionError: '无法解析：{token}',
    expressionHint: '共解析出 {count} 个 IOA',
    existingSameType: '已有 {count} 个同类型点位',
    conflictWarn: '与 {count} 个已存在 IOA 冲突，这些将被跳过',
    saving: '添加中...',
    add: '确认',
    failedPrefix: '批量添加失败：{err}',
    nextIoaBtn: '↓ 下一个可用 IOA',
    nextGapBtn: '↦ 跳到能放下的空隙',
    capacityFullTooltip: 'IOA 容量不足',
    conflictDetail: '冲突 IOA {ranges}（{count} 个点将被跳过）',
    crossTypeDup: '{count} 个 IOA 与本站其他类型共用：{ranges}。不阻断创建，但同一 CASDU 内 IOA 应唯一。',
  },
  batchControl: {
    title: '批量设置控制参数',
    selectionHint: '将应用到当前选中的 {count} 个控制点（位串命令不支持，将被跳过）。',
    applyQualifier: '设置 QU/QL 限定词',
    applySbo: '设置 S/E 执行模式',
    apply: '应用',
    appliedResult: '已更新 {applied}/{total} 个控制点',
    qualifierRange: '限定词超出范围（0..{max}）',
  },
  batchType: {
    title: '批量修改 ASDU 类型',
    selectionHint: '将迁移当前选中的 {count} 个监视点。',
    targetType: '目标 ASDU 类型',
    preserveHint: '仅可选择同一值族的兼容类型；运行值、品质、名称、备注、控制映射和活动模拟任务都会保留。',
    changedCount: '将迁移 {count} 个点位',
    apply: '迁移',
    appliedResult: '已迁移 {applied} 个点位',
    failed: '批量修改类型失败：{error}',
  },
  batchWrite: {
    title: '按 IOA 批量写值',
    typeLabel: '类型',
    ioaLabel: '目标 IOA',
    ioaPlaceholder: '如 100, 500, 1000-2000, 5000（逗号/空格/换行分隔）',
    valueLabel: '值',
    hit: '命中 {count} 个',
    ignored: '忽略 {count} 个',
    ignoredDetail: '忽略 {ranges}（不存在）',
    parseError: '无法解析：{token}',
    write: '写入',
    writeN: '写入 {count}',
    writing: '写入中…',
    failedPrefix: '批量写值失败：{err}',
    phSingle: '1/0 或 ON/OFF',
    phDouble: '0/1/2/3',
    phStep: '-64..63',
    phBitstring: 'u32 位串（十进制）',
    phNormalized: '原始 NVA 整数 -32768..32767',
    phScaled: 'i16 整数 -32768..32767',
    phFloat: '如 99.9',
    phTotal: 'i32 整数',
  },
  valuePanel: {
    title: '数据点详情',
    selectPointHint: '未选择数据点',
    selectPointHintSub: '在数据点表中点击任意一行查看详情',
    sectionInfo: '基本信息',
    asduType: 'ASDU 类型',
    category: '分类',
    name: '名称',
    comment: '备注',
    mapping: '控制映射',
    qualifierQoc: 'QOC / QU 限定词',
    qualifierQl: 'QL 限定词',
    qualifierAny: '任意（未限制）',
    qoc0: '0 — 无附加定义',
    qoc1: '1 — 短脉冲',
    qoc2: '2 — 长脉冲',
    qoc3: '3 — 持续输出',
    qocReserved: '{value} — 保留或厂商自定义',
    ql0: '0 — 无附加定义',
    qlOther: '{value} — 保留或应用自定义',
    executionMode: '执行模式 (S/E)',
    executionFlexible: '任意（直接执行或选择后执行）',
    executionDirect: '直接执行',
    executionSbo: '选择后执行 (SBO)',
    sectionCurrent: '当前值',
    value: '值',
    quality: '品质',
    qualityNa: '无品质 (N/A)',
    qualityValid: '正常',
    qualityInvalid: 'IV (无效)',
    timestamp: '时间戳',
    sectionWrite: '写入值',
    valuePlaceholder: '输入新值',
    write: '写入',
    sectionMultiSelect: '批量选中',
    countLabel: '数量',
    applyQuality: '应用品质',
    applyValue: '应用值',
    batchValueMixedHint: '仅同类型点位可批量写值',
  },
  quality: {
    legendTitle: '品质描述词 QDS · IEC 60870-5-101',
    bits: {
      iv: { name: '无效', desc: '值不可信 —— 采集/传感器故障' },
      nt: { name: '非现时', desc: '陈旧值 —— 数据源已失联' },
      sb: { name: '被取代', desc: '人工置数 —— 非现场采集' },
      bl: { name: '被闭锁', desc: '已闭锁 —— 停止刷新' },
      ov: { name: '溢出', desc: '超出量程 —— 仅测量类' },
    },
  },
  doublePoint: {
    legendTitle: '双点遥信 DPI · 双位置状态',
    tokens: {
      intermediate: '中间',
      off: 'OFF',
      on: 'ON',
      indeterminate: '不确定',
    },
    states: {
      intermediate: 'DPI=0 中间态 · 双位均为 0（动作过程 / 未定义）',
      off: 'DPI=1 分闸（断开）',
      on: 'DPI=2 合闸（闭合）',
      indeterminate: 'DPI=3 不确定态 · 双位均为 1（故障 / 矛盾指示）',
    },
  },
  log: {
    title: '通信日志',
    refresh: '刷新',
    clear: '清除',
    export: '导出 CSV',
    exporting: '导出中...',
    exportFailed: '导出 CSV 失败',
    loading: '加载中...',
    chooseServer: '请先选择一个服务器',
    noLogs: '暂无日志',
    noMatches: '没有符合当前筛选条件的日志',
    autoScroll: '自动滚动',
    allDirections: '全部方向',
    allFrames: '全部帧',
    iFrames: 'I 帧',
    sFrames: 'S 帧',
    uFrames: 'U 帧',
    searchPlaceholder: '搜索时间、方向、帧、详情或原始报文',
    filteredCount: '{visible} / {total}',
    resizeColumn: '调整“{column}”列宽',
    timeCol: '时间',
    directionCol: '方向',
    frameCol: '帧类型',
    detailCol: '详情',
    rawCol: '原始数据',
    titleRefresh: '刷新',
    titleClear: '清除',
    titleExport: '导出CSV',
    backendDetailFallback: '后端事件（技术上下文：{technical}）',
    serverStarted: '服务器已启动：{address}（{transport}）',
    serverStopped: '服务器已停止',
    cmdRejected: '{type} 已拒绝（原因={reason}，COT={cot}）IOA={ioa} CA={ca}',
    cmdCancel: '{type} 停止激活确认 IOA={ioa} CA={ca}',
    cmdSelect: '{type} 选择确认 IOA={ioa} QU/QL={qu} CA={ca}',
    cmdExecute: '{type} 已执行 val={val} QU/QL={qu} IOA={ioa} CA={ca} target={target}',
    cmdMappingBroken: '{type} 映射目标不存在 IOA={ioa} CA={ca}',
    cmdMalformed: '畸形控制报文 type={type} len={len} CA={ca}',
    unknownType: '未知 ASDU Type ID={type} 已拒绝（COT=44 否定确认）CA={ca} 原 COT={cot}',
    clockSyncDisabled: '时钟同步 拒收（应答已禁用，COT=7 + P/N） CA={ca}',
    clockSyncInvalidCot: '时钟同步 拒收（非法 COT={cot}，回 COT=45 + P/N） CA={ca}',
    clockSyncInvalidIoa: '时钟同步 拒收（IOA={ioa}，应为 0；回 COT=47 + P/N） CA={ca}',
    clockSyncMalformed: '时钟同步畸形 ASDU 已丢弃（原因={reason}，长度={len}） CA={ca}',
  },
  about: {
    whatsNew: '本次更新',
    homepage: '项目主页',
    onlineDemoLabel: '在线体验 SimLab',
    homepageLabel: '项目主页',
    releasesLabel: '历史版本',
    copiedSuffix: '已复制到剪贴板',
  },
  appDialog: {
    cancel: '取消',
    ok: '确定',
    titleAlert: '提示',
    titleConfirm: '确认',
    titlePrompt: '输入',
    backendMessageFallback: '后端操作失败（技术上下文：{technical}）',
  },
  errors: {
    invalidPort: '请输入有效的端口号 (1-65535)',
    invalidCa: '请输入有效的公共地址 (1-65534)',
    stationCaRunning: '修改公共地址前请先停止服务器；站名可以在运行中修改。',
    invalidIoa: '请输入有效的 IOA (0 ~ 16777215 的整数)',
    startBindInUse: '无法监听 {addr}：端口已被其他程序占用。请停止占用程序或更换端口。（系统错误 {osError}）',
    startBindDenied: '无法监听 {addr}：系统拒绝访问该端口。Windows 上常见原因是 Hyper-V/WSL2 保留端口段、安全软件或独占绑定；请尝试未保留的高位端口，并检查 “netsh interface ipv4 show excludedportrange protocol=tcp”。（系统错误 {osError}）',
    startBindUnavailable: '无法监听 {addr}：该地址不属于本机。请使用 0.0.0.0、127.0.0.1 或本机网卡地址。（系统错误 {osError}）',
    startFailed: '服务器启动失败：{message}',
  },
  update: {
    available: '检测到新版本',
    newVersion: '新版本 v{version} 可用',
    changelog: '更新说明',
    ready: '更新已在后台下载完成，可以安装。',
    installNow: '立即更新',
    installNextLaunch: '下次启动自动更新',
    skip: '跳过此版本',
    working: '正在处理…',
    failedTitle: '更新失败',
  },
  parseFrame: {
    title: '报文解析器',
    hint: '粘贴一段 IEC 60870-5-104 APDU 的十六进制字节,自动展开 APCI/ASDU/IOA 详情。支持空格、换行、逗号分隔。',
    hexLabel: '十六进制字节',
    templatesLabel: '模板:',
    errEmpty: '请输入 hex 报文',
    parse: '解析 (Ctrl+Enter)',
    parsing: '解析中...',
    apciI: 'I 帧 (Information)',
    apciS: 'S 帧 (Supervisory)',
    apciU: 'U 帧 · {name}',
    bytes: '{n} 字节',
    startByte: '起始字节',
    apduLength: 'APDU 长度',
    controlField: '控制字段',
    seqNo: '序列号',
    typeRow: '类型',
    cotNegative: 'P/N=否定',
    cotTest: 'T=测试',
    cotNames: {
      '1': '周期', '2': '背景扫描', '3': '自发', '4': '初始化',
      '5': '请求', '6': '激活', '7': '激活确认', '8': '停止激活',
      '9': '停止确认', '10': '激活终止', '20': '总召唤响应', '37': '计数量召唤响应',
    },
    oa: 'OA (源地址)',
    ca: 'CA (公共地址)',
    objects: '信息对象',
    objectsCount: '{n} 个',
    colValue: '值',
    colQuality: '品质',
    colTimestamp: '时间戳',
    colRaw: '原始字节',
    dpIntermediate: '中间',
    dpIndeterminate: '不确定',
  },
  remoteParams: {
    linkParams: '链路参数',
    linkParamsSub: '协议时序与窗口',
    hintT0: '建立连接超时',
    hintT1: '发送/测试超时',
    hintT2: 'S 帧响应超时',
    hintT3: 'TestFR 触发',
    hintK: '未确认 I 帧上限',
    hintW: '累计后回送 S 帧',
    autoCorrected: '已自动调整以满足约束 (t2<t1<t3, w≤⌊2k/3⌋):',
    interrogation: '召唤与应答',
    interrogationSub: '主站请求处理',
    answerSwitches: '应答开关',
    gi: '总召唤',
    counterInterrogation: '累积量召唤',
    commands: '遥控、遥调',
    clockSync: '时钟同步（对时）',
    clockSyncHint: '关闭不是静默不应答：收到 C_CS_NA_1 会回 COT=7（激活确认）并置 P/N 否定确认位——类型识别得了、请求结构也合法，只是按配置拒绝执行。抓包上每条请求仍能看到一帧应答。无论开关开或关，模拟器都不会调整本机时钟——该开关只决定如何应答。',
    sendActTerm: '执行后追加 ACT_TERM（激活终止帧）',
    sendActTermHint: '仅作用于遥控、遥调的执行应答。总召唤与累积量召唤的结束帧按 IEC 60870-5-104 要求仍为 COT=10，不受此开关影响。',
    executeCotDisabledHint: 'Execute COT 仅在 ACT_TERM 开启时生效；ACT_TERM 关闭后完全不发终止帧，子站只回 ACT_CON（COT=7）。',
    appLayerNote: '应用层字节数由 IEC 60870-5-104 固定：CA=2、IOA=3、COT=2（原因字节 + 源发地址 ORG）。应答帧回显主站发来的 ORG 字节；子站自发帧（突发上送、召唤数据、周期上送）ORG 固定为 0。',
    controlMappingHint: '控制点可在点位编辑器中独立映射到任意兼容的监视点。',
    autoMapCommands: '兼容模式：未显式映射时按相同 CA + IOA 自动映射',
    ackUnmappedCommands: '已声明但未映射的控制点仍正常应答 COT 7 → 10',
    sboEnforce: '强制选择后执行 (SBO)',
    sboTimeout: '选择有效期',
    giWithTimestamp: '召唤含带时标点',
    cmdAckCot: '命令应答 COT',
    select: '选择',
    execute: '执行',
    cancel: '取消',
    uploadMode: '数据上送方式',
    uploadModeSub: 'ASDU 组装策略',
    sqMode: 'SQ 模式',
    untimestamped: '不带时标',
    timestamped: '带时标',
    packingStrategy: '组包策略',
    autoPacking: '自动组包（连续 IOA 合并）',
    syncTb: '变位同步上送 TB（按分类）',
    syncTbNote: '开启后变位时在 NA 帧之后追加一帧对应 TB 类型；点位自身的 Type ID 不变，数据表类型列会显示 +TB 徽标提示。',
    mutationSim: '变位仿真',
    randomPacing: '点位仿真上送节流',
    pacingHint: '当前服务器全部站点共用，翻转、递增、递减、随机均生效。每个主站连接累计上送指定数量的点后等待；延迟 0 表示不等待。',
    pacingSaved: '已保存，运行中的仿真立即生效',
    perSend: '每发送',
    unitCount: '个',
    delay: '延迟',
    modeContinuous: '连续 SQ=1',
    modeDiscrete: '离散 SQ=0',
    connParams: '连接参数',
    connParamsSub: '监听地址与端口',
    bindAddress: '绑定地址',
    port: '端口',
    runningHint: '服务器运行中,地址 / 端口不可改 —— 请先在连接树右键「停止」',
    stopBeforeEdit: '请先停止服务器再修改监听地址 / 端口',
    drawerTitle: '远动运行参数',
    discard: '放弃',
    discardTitle: '放弃修改 · 重新载入',
    closeEsc: '关闭 (Esc)',
    loadingText: '载入中…',
    footNote: 't1/t2/t3 当前仅持久化，运行时计时器未完全驱动。',
    selectServerFirst: '请先在左侧选择一个服务器',
    saving: '保存中…',
    saved: '已保存',
    saveAll: '保存全部',
    configTimingCorrected: '加载配置时已自动调整时序以满足约束 (t2<t1<t3, w≤⌊2k/3⌋):',
  },
  _test: {
    interp: '订单 #{id} 由 {user} 创建',
  },
}

export default dict
