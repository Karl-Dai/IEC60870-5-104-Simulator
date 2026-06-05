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
  }
  toolbar: {
    newConnection: string
    connect: string
    disconnect: string
    delete: string
    sendGI: string
    clockSync: string
    counterRead: string
    customControl: string
    parseFrame: string
    parseFrameInLog: string
    checkUpdate: string
    checkingUpdate: string
    alreadyLatest: string
    updateCheckFailed: string
    updateCheckFailedMirrorPrompt: string
    appTitle: string
    about: string
    saveConfig: string
    openConfig: string
    configSaved: string
    configLoaded: string
    configSaveFailed: string
    configLoadFailed: string
    broadcast: string
    broadcastGi: string
    broadcastClockSync: string
    broadcastCounterRead: string
    broadcastAddressLabel: string
  }
  newConn: {
    title: string
    editTitle: string
    disconnectFirst: string
    targetAddress: string
    port: string
    commonAddress: string
    commonAddressHint: string
    invalidCA: string
    enableTls: string
    tlsVersion: string
    tlsAuto: string
    tls12: string
    tls13: string
    tls13WinWarn: string
    caFile: string
    certFile: string
    keyFile: string
    acceptInvalidCerts: string
    create: string
    protocolParams: string
    protocolParamsHint: string
    unitSeconds: string
    defaultQoi: string
    defaultQcc: string
    interrogatePeriod: string
    counterInterrogatePeriod: string
    timingCorrected: string
    broadcastAddress: string
    broadcastAddressHint: string
    broadcastAddressInvalid: string
  }
  tree: {
    title: string
    noConnections: string
    deleteConnection: string
    editConnection: string
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
  }
  table: {
    allData: string
    countSuffix: string
    countOf: string
    chooseConnection: string
    searchPlaceholder: string
    type: string
    value: string
    quality: string
    timestamp: string
    noDataHint: string
    setpoint: string
    copyIoa: string
    copyValue: string
    freeControl: string
  }
  valuePanel: {
    title: string
    selectPointHint: string
    selectedPoint: string
    type: string
    category: string
    value: string
    quality: string
    qualityValid: string
    qualityInvalid: string
    timestamp: string
    timestampNone: string
    quickControl: string
    sendSetpoint: string
    sboLabel: string
    sboTwoStep: string
    sboDirect: string
    notControllable: string
    doubleIntermediate: string
    doubleInvalid: string
  }
  quality: {
    legendTitle: string
    bits: Record<'iv' | 'nt' | 'sb' | 'bl' | 'ov', { name: string; desc: string }>
  }
  log: {
    title: string
    noConnections: string
    noLogs: string
    timeCol: string
    directionCol: string
    frameCol: string
    causeCol: string
    detailCol: string
    rawCol: string
    refresh: string
    clear: string
    export: string
    singleCommand: string
    doubleCommand: string
    stepCommand: string
    setpointNormalized: string
    setpointScaled: string
    setpointFloat: string
    frame: {
      iFrame: string
      sFrame: string
      uStartAct: string
      uStartCon: string
      uStopAct: string
      uStopCon: string
      uTestAct: string
      uTestCon: string
      generalInterrogation: string
      counterRead: string
      clockSync: string
      singleCommand: string
      doubleCommand: string
      setpointNormalized: string
      setpointScaled: string
      setpointFloat: string
      connectionEvent: string
    }
    cot: Record<string, string>
  }
  control: {
    title: string
    targetCa: string
    caCustom: string
    ioa: string
    commandType: string
    cmdSingle: string
    cmdDouble: string
    cmdStep: string
    cmdSetNorm: string
    cmdSetScaled: string
    cmdSetFloat: string
    cmdBitstring: string
    optOff: string
    optOn: string
    optIntermediate: string
    optOpen: string
    optClose: string
    optInvalid: string
    optStepDown: string
    optStepUp: string
    valueRangeScaled: string
    valueLabel: string
    valueRangeBitstring: string
    bitstringHex: string
    sboLabel: string
    sboTwoStep: string
    sboDirect: string
    bitstringNoSbo: string
    advancedSummary: string
    qulqlLabel: string
    qulqlIgnored: string
    qulqlSingle: string
    qulqlSetpoint: string
    qulqlBitstring: string
    cotLabel: string
    cot6: string
    cot7: string
    cot8: string
    cot9: string
    cot10: string
    sending: string
    send: string
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
    connectFailed: string
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
  },
  toolbar: {
    newConnection: '新建连接',
    connect: '连接',
    disconnect: '断开',
    delete: '删除',
    sendGI: '总召唤',
    clockSync: '时钟同步',
    counterRead: '累计量召唤',
    customControl: '自定义控制',
    parseFrame: '报文解析',
    parseFrameInLog: '解析此报文',
    checkUpdate: '检查更新',
    checkingUpdate: '检查中…',
    alreadyLatest: '已是最新版本',
    updateCheckFailed: '更新检查失败',
    updateCheckFailedMirrorPrompt: '更新检查失败,可能无法访问 GitHub。是否打开国内镜像下载页面?',
    appTitle: 'IEC104 Master',
    about: '关于',
    saveConfig: '保存配置',
    openConfig: '打开配置',
    configSaved: '配置已保存',
    configLoaded: '已导入 {count} 个连接',
    configSaveFailed: '保存失败',
    configLoadFailed: '打开失败',
    broadcast: '广播',
    broadcastGi: '广播总召',
    broadcastClockSync: '广播对时',
    broadcastCounterRead: '广播计量召唤',
    broadcastAddressLabel: '当前广播地址',
  },
  newConn: {
    title: '新建连接',
    editTitle: '编辑连接',
    disconnectFirst: '请先断开此连接再编辑',
    targetAddress: '目标地址',
    port: '端口',
    commonAddress: '公共地址 (CA)',
    commonAddressHint: '多个公共地址用逗号分隔，例如 1, 2, 3',
    invalidCA: '请填写至少一个有效的公共地址 (1–65534)',
    enableTls: '启用 TLS',
    tlsVersion: 'TLS 版本',
    tlsAuto: '自动',
    tls12: '仅 TLS 1.2',
    tls13: '仅 TLS 1.3',
    tls13WinWarn: '当前 Windows 的 SChannel 可能不支持 TLS 1.3(Windows 10/Server 2019 及更早均不支持,需 Windows 11/Server 2022+)。若握手失败,请改用「自动」或「仅 TLS 1.2」。',
    caFile: 'CA 证书路径',
    certFile: '客户端证书路径',
    keyFile: '客户端密钥路径',
    acceptInvalidCerts: '接受无效证书（测试用）',
    create: '创建',
    protocolParams: 'IEC 104 协议参数',
    protocolParamsHint: 't0/t1/t2/t3/k/w 遵循 IEC 60870-5-104 §5.2；周期为 0 时不进行自动召唤',
    unitSeconds: '秒',
    defaultQoi: '默认 QOI（总召唤限定词）',
    defaultQcc: '默认 QCC（计数量召唤限定词）',
    interrogatePeriod: '总召唤周期 (秒，0=关闭)',
    counterInterrogatePeriod: '计数量召唤周期 (秒，0=关闭)',
    timingCorrected: '已自动调整以满足约束 (t2<t1<t3, w≤⌊2k/3⌋): {detail}',
    broadcastAddress: '广播公共地址',
    broadcastAddressHint: '4 位十六进制，默认 FFFF；常见方言 FF00',
    broadcastAddressInvalid: '请输入 1-4 位十六进制',
  },
  tree: {
    title: '连接列表',
    noConnections: '暂无连接',
    deleteConnection: '删除连接',
    editConnection: '编辑连接',
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
  },
  table: {
    allData: '全部数据',
    countSuffix: '个',
    countOf: '/',
    chooseConnection: '选择一个连接查看数据',
    searchPlaceholder: '搜索 IOA / 类型...',
    type: '类型',
    value: '值',
    quality: '品质',
    timestamp: '时间戳',
    noDataHint: '暂无数据，请先发送总召唤',
    setpoint: '设定值...',
    copyIoa: '复制 IOA',
    copyValue: '复制值',
    freeControl: '自由控制...',
  },
  valuePanel: {
    title: '数据详情',
    selectPointHint: '选择数据点查看详情',
    selectedPoint: '选中数据点',
    type: '类型',
    category: '分类',
    value: '值',
    quality: '品质',
    qualityValid: 'OK (有效)',
    qualityInvalid: 'IV (无效)',
    timestamp: '时间戳',
    timestampNone: '无',
    quickControl: '快捷控制',
    sendSetpoint: '发送设定值',
    sboLabel: '选择-执行 (SbO)',
    sboTwoStep: '自动两步',
    sboDirect: '直接执行',
    notControllable: '此类型不支持控制操作',
    doubleIntermediate: '中间',
    doubleInvalid: '不确定',
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
    noConnections: '暂无连接',
    noLogs: '暂无日志',
    timeCol: '时间',
    directionCol: '方向',
    frameCol: '帧类型',
    causeCol: '传送原因',
    detailCol: '详情',
    rawCol: '原始数据',
    refresh: '刷新',
    clear: '清空',
    export: '导出',
    singleCommand: '单点命令 IOA={ioa} val={val}',
    doubleCommand: '双点命令 IOA={ioa} val={val}',
    stepCommand: '步调节命令 IOA={ioa} val={val}',
    setpointNormalized: '归一化设定值 IOA={ioa} val={val}',
    setpointScaled: '标度化设定值 IOA={ioa} val={val}',
    setpointFloat: '浮点设定值 IOA={ioa} val={val}',
    frame: {
      iFrame: 'I {value}',
      sFrame: 'S 帧',
      uStartAct: 'U 启动激活',
      uStartCon: 'U 启动确认',
      uStopAct: 'U 停止激活',
      uStopCon: 'U 停止确认',
      uTestAct: 'U 测试激活',
      uTestCon: 'U 测试确认',
      generalInterrogation: '总召',
      counterRead: '计数召唤',
      clockSync: '对时',
      singleCommand: '单点命令',
      doubleCommand: '双点命令',
      setpointNormalized: '归一设定值',
      setpointScaled: '标度设定值',
      setpointFloat: '浮点设定值',
      connectionEvent: '连接事件',
    },
    cot: {
      unknown: '未知({cot})',
      '1': '周期/循环',
      '2': '背景扫描',
      '3': '突发',
      '4': '初始化',
      '5': '请求',
      '6': '激活',
      '7': '激活确认',
      '8': '停止激活',
      '9': '停止激活确认',
      '10': '激活终止',
      '11': '远方命令引起的返回信息',
      '12': '本地命令引起的返回信息',
      '13': '文件传输',
      '20': '响应总召',
      '21': '响应第1组召唤',
      '22': '响应第2组召唤',
      '23': '响应第3组召唤',
      '24': '响应第4组召唤',
      '25': '响应第5组召唤',
      '26': '响应第6组召唤',
      '27': '响应第7组召唤',
      '28': '响应第8组召唤',
      '29': '响应第9组召唤',
      '30': '响应第10组召唤',
      '31': '响应第11组召唤',
      '32': '响应第12组召唤',
      '33': '响应第13组召唤',
      '34': '响应第14组召唤',
      '35': '响应第15组召唤',
      '36': '响应第16组召唤',
      '37': '响应计数器总召',
      '38': '响应第1组计数器召唤',
      '39': '响应第2组计数器召唤',
      '40': '响应第3组计数器召唤',
      '41': '响应第4组计数器召唤',
      '44': '未知类型标识',
      '45': '未知传送原因',
      '46': '未知公共地址',
      '47': '未知信息对象地址',
    },
  },
  control: {
    title: '发送控制命令',
    targetCa: '目标公共地址 (CA)',
    caCustom: '其他 (手动输入)',
    ioa: 'IOA (信息对象地址)',
    commandType: '命令类型',
    cmdSingle: '单点命令 (C_SC_NA_1 · 45)',
    cmdDouble: '双点命令 (C_DC_NA_1 · 46)',
    cmdStep: '步调节命令 (C_RC_NA_1 · 47)',
    cmdSetNorm: '归一化设定值 (C_SE_NA_1 · 48)',
    cmdSetScaled: '标度化设定值 (C_SE_NB_1 · 49)',
    cmdSetFloat: '浮点设定值 (C_SE_NC_1 · 50)',
    cmdBitstring: '位串命令 (C_BO_NA_1 · 51)',
    optOff: '分闸 OFF',
    optOn: '合闸 ON',
    optIntermediate: '中间',
    optOpen: '分',
    optClose: '合',
    optInvalid: '不确定',
    optStepDown: '降',
    optStepUp: '升',
    valueRangeScaled: '值 (-32768 ~ 32767)',
    valueLabel: '值',
    valueRangeBitstring: '值 (32 位无符号)',
    bitstringHex: '十六进制',
    sboLabel: '选择-执行 (SbO)',
    sboTwoStep: '自动两步',
    sboDirect: '直接执行',
    bitstringNoSbo: '位串命令不支持 SbO',
    advancedSummary: '高级参数 (QU/QL/COT)',
    qulqlLabel: 'QU/QL 限定词',
    qulqlIgnored: 'QU/QL (忽略)',
    qulqlSingle: 'QU: 0=无附加定义, 1=短脉冲, 2=长脉冲, 3=持续 (写入命令字节 bit2..6)',
    qulqlSetpoint: 'QL: 0..127 (写入 QOS 低 7 位)',
    qulqlBitstring: '位串命令无 QU/QL,本字段忽略',
    cotLabel: 'COT 传送原因',
    cot6: '6 - 激活',
    cot7: '7 - 激活确认',
    cot8: '8 - 停止激活',
    cot9: '9 - 停止激活确认',
    cot10: '10 - 激活终止',
    sending: '发送中...',
    send: '发送',
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
    connectFailed: '连接失败: {err}\n将每 {sec} 秒自动重试,点击「断开」可停止。',
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
  _test: {
    interp: '订单 #{id} 由 {user} 创建',
  },
}

export default dict
