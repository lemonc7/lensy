// 导入obsidian api
const {
  Notice,
  Plugin,
  PluginSettingTab,
  SecretComponent,
  Setting,
  requestUrl,
} = require("obsidian");

// 配置参数 
const DEFAULT_SETTINGS = {
  serverUrl: "",
  tokenSecretName: "",
};

class LensyUploaderPlugin extends Plugin {
  // 插件加载，启动插件时会调用onload()
  async onload() {
    // 加载服务器地址等设置
    this.settings = Object.assign(
      {}, 
      DEFAULT_SETTINGS, 
      await this.loadData(),
    );

    // 注册设置页面
    this.addSettingTab(new LensySettingTab(this.app, this));

    // 监听编辑器的editor-paste时间
    this.registerEvent(
      this.app.workspace.on("editor-paste", (event, editor) => {
        void this.handlePaste(event, editor);
      }),
    );
  }

  async saveSettings() {
    await this.saveData(this.settings);
  }

  // 识别剪贴板图片
  async handlePaste(event, editor) {
    if (event.defaultPrevented) return;

    
    // 只接管jpeg/png/webp三种格式
    const files = Array.from(event.clipboardData?.files ?? [])
      .filter((file) =>
        ["image/jpeg", "image/png", "image/webp"].includes(file.type),
    );

    if (files.length === 0) return;

    // 阻止obsidian将图片保存为本地附件，由插件接管后续上传
    event.preventDefault();

    // 插入临时唯一占位符
    const jobs = files.map((file) => ({
      file,
      marker: `<!-- lensy-upload:${crypto.randomUUID()} -->`,
    }));
    // 立即插入文档，等待异步上传成功再替换
    editor.replaceSelection(jobs.map((job) => job.marker).join("\n\n"));

    // 获取地址和token
    let serverUrl;
    try {
      // 校验server_url连接
      serverUrl = normalizeServerUrl(this.settings.serverUrl);
    } catch (error) {
      this.failAll(editor, jobs, errorMessage(error));
      return;
    }

    // 从obsidian secret storage中获取
    const token = this.app.secretStorage.getSecret(this.settings.tokenSecretName);
    if (!token) {
      this.failAll(editor, jobs, "尚未配置 Lensy 上传 Token");
      return;
    }

    // 串行上传，保持多图顺序并限制客户端内存占用。
    for (const job of jobs) {
      try {
        const uploaded = await uploadImage(serverUrl, token, job.file);
        const publicUrl = `${serverUrl}/i/${encodeURIComponent(uploaded.image.public_id)}`;
        const markdown = `![${escapeMarkdownAlt(job.file.name || "image")}](${publicUrl})`;
        replaceMarker(editor, job.marker, markdown);
      } catch (error) {
        const message = errorMessage(error);
        replaceMarker(editor, job.marker, uploadErrorMarkdown(message));
        new Notice(`Lensy 上传失败：${message}`, 8000);
      }
    }
  }

  failAll(editor, jobs, message) {
    for (const job of jobs) {
      replaceMarker(editor, job.marker, uploadErrorMarkdown(message));
    }
    new Notice(`Lensy 上传失败：${message}`, 8000);
  }
}

// 设置页面
class LensySettingTab extends PluginSettingTab {
  constructor(app, plugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display() {
    const { containerEl } = this;
    containerEl.empty();

    new Setting(containerEl)
      .setName("Lensy 地址")
      .setDesc("例如 https://image.example.com，不需要填写接口路径。")
      .addText((text) =>
        text
          .setPlaceholder("https://image.example.com")
          .setValue(this.plugin.settings.serverUrl)
          .onChange(async (value) => {
            this.plugin.settings.serverUrl = value.trim();
            await this.plugin.saveSettings();
          }),
      );

    new Setting(containerEl)
      .setName("上传 Token")
      .setDesc("创建或选择一个 Secret，值应与 Lensy auth.token 一致。")
      .addComponent((element) =>
        new SecretComponent(this.app, element)
          .setValue(this.plugin.settings.tokenSecretName)
          .onChange(async (value) => {
            this.plugin.settings.tokenSecretName = value;
            await this.plugin.saveSettings();
          }),
      );
  }
}

// 构造http请求，上传图片
async function uploadImage(serverUrl, token, file) {
  const response = await requestUrl({
    url: `${serverUrl}/api/v1/images`,
    method: "POST",
    contentType: file.type,
    headers: {
      Authorization: `Bearer ${token}`,
      "X-Filename": clipboardFilename(file.type),
    },
    body: await file.arrayBuffer(),
    throw: false,
  });

  if (response.status < 200 || response.status >= 300) {
    throw new Error(`HTTP ${response.status}`);
  }

  const result = response.json;
  if (!result?.image?.public_id) {
    throw new Error("Lensy 返回了无效的上传结果");
  }
  return result;
}

function normalizeServerUrl(value) {
  const url = new URL(value.trim());
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Lensy 地址必须使用 HTTP 或 HTTPS");
  }
  if (url.username || url.password || url.search || url.hash) {
    throw new Error("Lensy 地址不能包含凭据、查询参数或片段");
  }

  url.pathname = url.pathname.replace(/\/+$/, "");
  return url.toString().replace(/\/+$/, "");
}

// 生成文件名
function clipboardFilename(mime) {
  const extension = mime === "image/jpeg" ? "jpg" : mime === "image/webp" ? "webp" : "png";
  const timestamp = new Date().toISOString().replace(/\D/g, "").slice(0, 17);
  return `${timestamp}.${extension}`;
}

// 替换占位符
function replaceMarker(editor, marker, replacement) {
  // 读取当前文档
  const content = editor.getValue();
  // 查找对应占位符
  const offset = content.indexOf(marker);
  if (offset < 0) return;

  // 仅替换该占位符
  editor.replaceRange(
    replacement,
    editor.offsetToPos(offset),
    editor.offsetToPos(offset + marker.length),
  );
}

function uploadErrorMarkdown(message) {
  return `**Lensy 图片上传失败：${escapeMarkdownText(message)}**`;
}

function escapeMarkdownAlt(value) {
  return value.replaceAll("\\", "\\\\").replaceAll("]", "\\]");
}

function escapeMarkdownText(value) {
  return value
    .replace(/\r?\n/g, " ")
    .replaceAll("\\", "\\\\")
    .replaceAll("*", "\\*")
    .replaceAll("_", "\\_");
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

module.exports = LensyUploaderPlugin;
