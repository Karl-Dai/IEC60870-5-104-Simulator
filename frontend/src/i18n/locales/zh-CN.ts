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
  }
  toolbar: {
    newServer: string
    start: string
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
    configSaved: string
    configLoaded: string
    configSaveFailed: string
    configLoadFailed: string
  }
  newServer: {
    title: string
    bindAddressLabel: string
    bindAddressHint: string
    portLabel: string
    initMode: string
    initZero: string
    initRandom: string
    countPerCategory: string
    enableTls: string
    serverCert: string
    serverKey: string
    caFile: string
    requireClientCert: string
  }
  prompt: {
    inputCommonAddress: string
    inputStationName: string
    defaultStationName: string
  }
  station: {
    defaultName: string
  }
  tree: {
    title: string
    noServers: string
    noServersHint: string
    ctxStartServer: string
    ctxStopServer: string
    ctxDeleteServer: string
    ctxDeleteStation: string
    ctxEditRuntimeParams: string
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
    mutationStep: string
    mutationMin: string
    mutationMax: string
    derivedTbTitle: string
    batchControlOptions: string
  }
  pointModal: {
    title: string
    editTitle: string
    ioaLabel: string
    ioaPlaceholder: string
    ioaEditHint: string
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
    timeCol: string
    directionCol: string
    frameCol: string
    detailCol: string
    rawCol: string
    titleRefresh: string
    titleClear: string
    titleExport: string
  }
  about: {
    whatsNew: string
    homepage: string
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
  }
  errors: {
    invalidPort: string
    invalidCa: string
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
    installNow: string
    later: string
    downloading: string
    failedTitle: string
    retry: string
    close: string
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
  },
  toolbar: {
    newServer: '新建服务器',
    start: '启动',
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
    configSaved: '配置已保存',
    configLoaded: '已导入 {count} 个服务器',
    configSaveFailed: '保存失败',
    configLoadFailed: '打开失败',
  },
  newServer: {
    title: '新建服务器',
    bindAddressLabel: '监听地址',
    bindAddressHint: '0.0.0.0 监听全部网卡；也可输入指定网卡的 IP 地址。',
    portLabel: '端口号',
    initMode: '初始值',
    initZero: '全零',
    initRandom: '随机',
    countPerCategory: '每类点数',
    enableTls: '启用 TLS',
    serverCert: '服务器证书文件 (PEM)',
    serverKey: '服务器密钥文件 (PEM)',
    caFile: 'CA 证书文件 (PEM, 可选)',
    requireClientCert: '要求客户端证书 (mTLS)',
  },
  prompt: {
    inputCommonAddress: '输入公共地址 (CA)',
    inputStationName: '输入站名',
    defaultStationName: '站 {ca}',
  },
  station: {
    defaultName: '站 {ca}',
  },
  tree: {
    title: '服务器',
    noServers: '暂无服务器',
    noServersHint: '点击左上角「+ 新建服务器」开始',
    ctxStartServer: '启动服务器',
    ctxStopServer: '停止服务器',
    ctxDeleteServer: '删除服务器',
    ctxDeleteStation: '删除站',
    ctxEditRuntimeParams: '修改运行参数',
    connTooltip: '已连接 {n} 个客户端',
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
    summary: '{n} 个客户端已连接',
    empty: '暂无客户端连接',
    emptyHint: '服务器运行后，master 连接将显示在此处',
    stateActive: '数据传输中',
    stateConnected: '已连接',
    colPeer: '客户端地址',
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
    batchAdd: '批量',
    batchWrite: '写值',
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
    mutationStep: '步长',
    mutationMin: '下限',
    mutationMax: '上限',
    derivedTbTitle: '变位时将追加派生帧 {tb}（「变位同步上送 TB」已开启；点位自身 Type ID 不变）',
    batchControlOptions: '批量设置控制参数',
  },
  pointModal: {
    title: '添加数据点',
    editTitle: '编辑数据点',
    ioaLabel: 'IOA (信息对象地址)',
    ioaPlaceholder: '例如: 100',
    ioaEditHint: '可修改 IOA 改址；运行值与品质保留，引用本点的控制映射会同步更新。',
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
  log: {
    title: '通信日志',
    refresh: '刷新',
    clear: '清除',
    export: '导出CSV',
    exporting: '导出中...',
    exportFailed: '导出 CSV 失败',
    loading: '加载中...',
    chooseServer: '请先选择一个服务器',
    noLogs: '暂无日志',
    timeCol: '时间',
    directionCol: '方向',
    frameCol: '帧类型',
    detailCol: '详情',
    rawCol: '原始数据',
    titleRefresh: '刷新',
    titleClear: '清除',
    titleExport: '导出CSV',
  },
  about: {
    whatsNew: '本次更新',
    homepage: '项目主页',
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
  },
  errors: {
    invalidPort: '请输入有效的端口号 (1-65535)',
    invalidCa: '请输入有效的公共地址 (1-65534)',
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
    installNow: '立即更新',
    later: '稍后',
    downloading: '正在下载 {pct}%',
    failedTitle: '更新失败',
    retry: '重试',
    close: '关闭',
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
    randomPacing: '随机变位节流',
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
