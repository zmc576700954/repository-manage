# GitHub → Gitee 自动镜像同步

让 GitHub Desktop 推送到 GitHub 后，Gitee 自动同步。**只需一次配置**，之后完全自动。

## 前提条件

1. GitHub 仓库 `zmc576700954/repository-manage` 是 **Public**（Private 仓库无法被 Gitee 拉取）
2. 你有 Gitee 账号 `zhu_ming_chen` 的管理权限

---

## 步骤 1：GitHub 仓库设为 Public

在 GitHub 仓库页面：

1. 右上角 **Settings** → 左侧最下面 **General**
2. 滚到最下面 **Danger Zone** → **Change repository visibility** → **Make public**
3. 输入仓库名确认

> ⚠️ 如果仓库有敏感信息，先清理再公开

---

## 步骤 2：在 Gitee 端配置镜像

### 2.1 登录 Gitee

打开 https://gitee.com/zhu_ming_chen/repository-manage

### 2.2 创建镜像仓库

**如果你的 Gitee 仓库已经是空的**（即我从本地推上去的状态）：

1. 进入仓库页面 → 右上角 **管理** → 左侧 **基本信息**
2. 滚到最下面 **仓库镜像** 部分
3. 点击 **添加镜像** 按钮
4. 填写：
   - **镜像源 URL**：`https://github.com/zmc576700954/repository-manage.git`
   - **镜像类型**：选择 **全量同步**
   - **认证方式**：公开仓库（无需认证）
5. 点击 **确定**

### 2.3 启用自动同步

1. 同一页面找到 **同步方式** 设置
2. 勾选 **自动同步**
3. 选择 **推送触发** 或 **定时拉取**（推荐两者都开）
4. 保存

### 2.4 删除我之前手动推的内容（可选）

如果想彻底交由镜像管理：
- 在 Gitee 仓库管理页面 → **清空仓库** → 重新由镜像同步过来

---

## 步骤 3：配置 GitHub Desktop（一次性）

打开 GitHub Desktop：

1. **Repository** → **Repository Settings**
2. **Primary remote repository (origin)**：保持 `https://github.com/zmc576700954/repository-manage.git`
3. 关闭弹窗

之后你正常用 GitHub Desktop **Commit + Push origin** 即可。

---

## 验证流程

1. 在本地改一个文件
2. GitHub Desktop → Commit → Push origin
3. 打开 https://github.com/zmc576700954/repository-manage 看新 commit
4. 等 30 秒 ~ 2 分钟
5. 打开 https://gitee.com/zhu_ming_chen/repository-manage 应该看到相同的 commit

---

## 常见问题

### 镜像同步失败

进入 Gitee 仓库 → 管理 → 仓库镜像 → 点击 **立即同步** 按钮，查看错误信息。

### GitHub 仓库是 Private

镜像服务无法拉取 Private 仓库。三种方案：

1. **改为 Public**（推荐，如果有外部用户能查看仓库）
2. **付费**：Gitee 企业版支持 Private 仓库镜像
3. **改用 GitHub Actions**：用 PAT 推送到 Gitee，需要额外配置

### 想要 GitHub Actions 推送（额外保障）

如果你担心 Gitee 镜像服务宕机，可以在 GitHub 加个 Actions workflow 作为备份。仓库根目录创建 `.github/workflows/mirror-to-gitee.yml`：

```yaml
name: Mirror to Gitee
on:
  push:
    branches: [main]

jobs:
  mirror:
    runs-on: ubuntu-latest
    steps:
      - name: Push to Gitee
        uses: pixta-dev/repository-mirroring-action@v1
        with:
          target_repo_url: https://gitee.com/zhu_ming_chen/repository-manage.git
          ssh_private_key: ${{ secrets.GITEE_SSH_KEY }}
```

需要在 Gitee 生成 SSH 密钥对，公钥贴到 Gitee → 设置 → SSH 公钥，私钥存到 GitHub 仓库 secrets。

---

## 当前状态

| 仓库 | URL | 状态 |
|---|---|---|
| GitHub | https://github.com/zmc576700954/repository-manage | 待设为 Public |
| Gitee | https://gitee.com/zhu_ming_chen/repository-manage | 待配置镜像 |

镜像配好后，**本地不再需要任何手动同步脚本**。
