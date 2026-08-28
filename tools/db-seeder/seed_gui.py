"""智慧路灯测试数据工具 —— PySide6 GUI。

直连本地 PostgreSQL(不走后端 API / 鉴权),自动内省表结构:
  页签1 手动插入:按列动态生成表单,单行 INSERT
  页签2 批量插入:每列配生成策略,批量 executemany(进度条 / 可取消)
  页签3 场景预设:光照历史曲线回填、设备上下线(含离线告警语义)

用法:uv run seed_gui.py
字体:Qt 走 fontconfig,中文自动回退(不用 tkinter——uv 独立 Python 的 Tk 无
Xft/fontconfig 支持,看不到 CJK 字体);WSLg 下强制 wayland 平台,缩放由合成器处理。
"""

from __future__ import annotations

import os
import sys
import threading
from pathlib import Path
from urllib.parse import urlparse

from PySide6.QtCore import QObject, Qt, Signal
from PySide6.QtWidgets import (
    QApplication, QCheckBox, QComboBox, QGridLayout, QGroupBox, QHBoxLayout,
    QLabel, QLineEdit, QListWidget, QMainWindow, QMessageBox, QProgressBar,
    QPushButton, QSpinBox, QSplitter, QTabWidget, QVBoxLayout, QWidget,
)

import dbcore

DEFAULTS = {"host": "127.0.0.1", "port": "5432", "dbname": "streetlight",
            "user": "streetlight", "password": "streetlight"}

# backend/.env 候选位置(工具在 tools/db-seeder/ 下)
ENV_CANDIDATES = [
    Path(__file__).resolve().parents[2] / "backend" / ".env",
    Path.cwd() / "backend" / ".env",
]


class Bridge(QObject):
    """worker 线程 → GUI 线程的结果/进度投递(Qt 信号自动跨线程排队)。"""

    done = Signal(object)      # (on_done 回调, 结果, 异常)
    progress = Signal(int, int)


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("智慧路灯 · 测试数据工具(直连数据库)")
        self.resize(1160, 760)

        self.conn = None
        self.db_lock = threading.Lock()
        self.bridge = Bridge()
        self.bridge.done.connect(self._dispatch)
        self.bridge.progress.connect(self._on_progress)
        self.tables: dict[str, dbcore.Table] = {}
        self.current: dbcore.Table | None = None
        self.distinct: dict[str, list[str]] = {}
        self.manual_rows: list[dict] = []
        self.batch_rows: dict[str, tuple[QComboBox, QLineEdit]] = {}
        self.cancel_event: threading.Event | None = None

        self._build_conn_area()
        self._build_main_area()

    # ------------------------------------------------------------ UI 搭建

    def _build_conn_area(self):
        box = QGroupBox("数据库连接")
        grid = QGridLayout(box)
        self.conn_edits: dict[str, QLineEdit] = {}
        for col, (label, key, width) in enumerate([
            ("主机", "host", 150), ("端口", "port", 60), ("数据库", "dbname", 110),
            ("用户", "user", 110), ("密码", "password", 110),
        ]):
            grid.addWidget(QLabel(label), 0, col * 2)
            e = QLineEdit(DEFAULTS[key])
            e.setMinimumWidth(width)
            e.setMaximumWidth(width + 40)
            if key == "password":
                e.setEchoMode(QLineEdit.Password)
            grid.addWidget(e, 0, col * 2 + 1)
            self.conn_edits[key] = e
        btn_env = QPushButton("从 backend/.env 导入")
        btn_env.clicked.connect(self._import_env)
        self.btn_conn = QPushButton("连接")
        self.btn_conn.clicked.connect(self._connect)
        btn_disc = QPushButton("断开")
        btn_disc.clicked.connect(self._disconnect)
        self.conn_status = QLabel("● 未连接")
        self.conn_status.setStyleSheet("color:#c0392b")
        grid.addWidget(btn_env, 0, 10)
        grid.addWidget(self.btn_conn, 0, 11)
        grid.addWidget(btn_disc, 0, 12)
        grid.addWidget(self.conn_status, 0, 13)
        grid.setColumnStretch(14, 1)

        root = QVBoxLayout()
        root.addWidget(box)
        self._central_layout = root

    def _build_main_area(self):
        splitter = QSplitter(Qt.Horizontal)

        left = QWidget()
        lv = QVBoxLayout(left)
        lv.setContentsMargins(0, 0, 0, 0)
        lv.addWidget(QLabel("数据表(自动检测)"))
        self.table_list = QListWidget()
        self.table_list.currentTextChanged.connect(self._on_table_select)
        lv.addWidget(self.table_list, 1)
        btn_refresh = QPushButton("刷新表列表")
        btn_refresh.clicked.connect(self._refresh_tables)
        lv.addWidget(btn_refresh)
        splitter.addWidget(left)

        self.tabs = QTabWidget()
        self._build_manual_tab()
        self._build_batch_tab()
        self._build_preset_tab()
        splitter.addWidget(self.tabs)
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        splitter.setSizes([220, 900])

        self._central_layout.addWidget(splitter, 1)
        central = QWidget()
        central.setLayout(self._central_layout)
        self.setCentralWidget(central)

    # -- 页签 1:手动插入

    def _build_manual_tab(self):
        tab = QWidget()
        v = QVBoxLayout(tab)
        self.manual_form = QGridLayout()
        self.manual_form.setColumnStretch(3, 1)
        v.addLayout(self.manual_form, 1)
        bottom = QHBoxLayout()
        btn = QPushButton("插入一行")
        btn.clicked.connect(self._do_manual_insert)
        self.manual_result = QLabel("")
        bottom.addWidget(btn)
        bottom.addWidget(self.manual_result, 1)
        v.addLayout(bottom)
        self.tabs.addTab(tab, "手动插入")
        self._show_hint(self.manual_form, "先在左侧选择一张表")

    # -- 页签 2:批量插入

    def _build_batch_tab(self):
        tab = QWidget()
        v = QVBoxLayout(tab)
        hint = QLabel(dbcore.STRATEGY_HINT)
        hint.setStyleSheet("color:#555")
        hint.setWordWrap(True)
        v.addWidget(hint)
        self.batch_form = QGridLayout()
        self.batch_form.setColumnStretch(3, 1)
        v.addLayout(self.batch_form, 1)
        bottom = QHBoxLayout()
        bottom.addWidget(QLabel("行数"))
        self.batch_n = QSpinBox()
        self.batch_n.setRange(1, 10_000_000)
        self.batch_n.setValue(1000)
        bottom.addWidget(self.batch_n)
        self.btn_batch = QPushButton("开始批量插入")
        self.btn_batch.clicked.connect(self._do_batch)
        bottom.addWidget(self.btn_batch)
        self.btn_batch_cancel = QPushButton("取消")
        self.btn_batch_cancel.setEnabled(False)
        self.btn_batch_cancel.clicked.connect(self._cancel_batch)
        bottom.addWidget(self.btn_batch_cancel)
        self.prog = QProgressBar()
        self.prog.setMinimumWidth(240)
        bottom.addWidget(self.prog, 1)
        self.prog_label = QLabel("")
        bottom.addWidget(self.prog_label)
        v.addLayout(bottom)
        self.tabs.addTab(tab, "批量插入")
        self._show_hint(self.batch_form, "先在左侧选择一张表")

    # -- 页签 3:场景预设

    def _build_preset_tab(self):
        tab = QWidget()
        v = QVBoxLayout(tab)

        f1 = QGroupBox("光照历史曲线(回填 lux_record,设备置 online)")
        g = QGridLayout(f1)
        self.preset_device = QComboBox()
        self.preset_device.setEditable(True)
        self.preset_device.setMinimumWidth(220)
        self.preset_new = QCheckBox("新建设备")
        self.preset_new.toggled.connect(self._toggle_new_device)
        self.preset_new_id = QLineEdit("seed-lamp-1")
        self.preset_name = QLineEdit("测试路灯")
        self.preset_loc = QLineEdit("测试路段")
        self.preset_days = QSpinBox()
        self.preset_days.setRange(1, 90)
        self.preset_days.setValue(7)
        self.preset_step = QSpinBox()
        self.preset_step.setRange(10, 3600)
        self.preset_step.setValue(300)
        g.addWidget(QLabel("设备"), 0, 0)
        g.addWidget(self.preset_device, 0, 1)
        g.addWidget(self.preset_new, 0, 2)
        g.addWidget(QLabel("新设备ID"), 0, 3)
        g.addWidget(self.preset_new_id, 0, 4)
        g.addWidget(QLabel("名称"), 0, 5)
        g.addWidget(self.preset_name, 0, 6)
        g.addWidget(QLabel("位置"), 0, 7)
        g.addWidget(self.preset_loc, 0, 8)
        g.addWidget(QLabel("天数"), 1, 0)
        g.addWidget(self.preset_days, 1, 1)
        g.addWidget(QLabel("间隔秒"), 1, 2)
        g.addWidget(self.preset_step, 1, 3)
        btn_lux = QPushButton("生成光照历史")
        btn_lux.clicked.connect(self._do_lux_preset)
        g.addWidget(btn_lux, 1, 4)
        g.setColumnStretch(9, 1)
        v.addWidget(f1)
        self._toggle_new_device(False)

        f2 = QGroupBox("设备上下线(离线产生告警 / 上线自动消解)")
        h = QHBoxLayout(f2)
        h.addWidget(QLabel("设备"))
        self.preset_device2 = QComboBox()
        self.preset_device2.setEditable(True)
        self.preset_device2.setMinimumWidth(220)
        h.addWidget(self.preset_device2)
        btn_off = QPushButton("置为离线")
        btn_off.clicked.connect(lambda: self._do_online(False))
        h.addWidget(btn_off)
        btn_on = QPushButton("置为在线")
        btn_on.clicked.connect(lambda: self._do_online(True))
        h.addWidget(btn_on)
        self.preset_result = QLabel("")
        h.addWidget(self.preset_result, 1)
        v.addWidget(f2)

        note = QLabel(
            "说明:本工具直连数据库,不经过后端与华为云。测试设备不在 IoTDA 云端,"
            "后端轮询日志出现北向 404 属预期;compose 设 IOTHUB_DRY_RUN=true 可短路北向。")
        note.setStyleSheet("color:#555")
        note.setWordWrap(True)
        v.addWidget(note)
        v.addStretch(1)
        self.tabs.addTab(tab, "场景预设")

    # ------------------------------------------------------------ 后台执行

    def run_bg(self, work, on_done):
        def runner():
            try:
                res = work()
            except Exception as e:  # noqa: BLE001 - GUI 统一弹错
                self.bridge.done.emit((on_done, None, e))
            else:
                self.bridge.done.emit((on_done, res, None))
        threading.Thread(target=runner, daemon=True).start()

    def _dispatch(self, item):
        on_done, res, err = item
        on_done(res, err)

    def _on_progress(self, done: int, total: int):
        if total > 0:
            self.prog.setMaximum(total)
            self.prog.setValue(done)
        self.prog_label.setText(f"{done}/{total or '?'}")

    # ------------------------------------------------------------ 连接

    def _import_env(self):
        for p in ENV_CANDIDATES:
            if p.exists():
                break
        else:
            QMessageBox.warning(self, "未找到", "未找到 backend/.env")
            return
        for line in p.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line.startswith("DATABASE_URL="):
                u = urlparse(line.split("=", 1)[1].strip().strip('"').strip("'"))
                self.conn_edits["host"].setText(u.hostname or "")
                self.conn_edits["port"].setText(str(u.port or 5432))
                self.conn_edits["dbname"].setText(u.path.lstrip("/"))
                self.conn_edits["user"].setText(u.username or "")
                self.conn_edits["password"].setText(u.password or "")
                QMessageBox.information(self, "已导入", f"已从 {p} 导入 DATABASE_URL")
                return
        QMessageBox.warning(self, "未找到", f"{p} 中没有 DATABASE_URL")

    def _connect(self):
        try:
            port = int(self.conn_edits["port"].text())
        except ValueError:
            QMessageBox.critical(self, "参数错误", "端口必须是数字")
            return
        host = self.conn_edits["host"].text().strip()
        dbname = self.conn_edits["dbname"].text().strip()
        user = self.conn_edits["user"].text().strip()
        password = self.conn_edits["password"].text()
        self.btn_conn.setEnabled(False)
        self.conn_status.setText("● 连接中…")
        self.conn_status.setStyleSheet("color:#e67e22")

        def work():
            conn = dbcore.connect(host, port, dbname, user, password)
            with self.db_lock:
                tables = dbcore.list_tables(conn)
            return conn, tables

        def done(res, err):
            self.btn_conn.setEnabled(True)
            if err:
                self.conn_status.setText("● 连接失败")
                self.conn_status.setStyleSheet("color:#c0392b")
                QMessageBox.critical(self, "连接失败", str(err))
                return
            self.conn, tables = res
            self.conn_status.setText("● 已连接")
            self.conn_status.setStyleSheet("color:#27ae60")
            self._fill_tables(tables)
        self.run_bg(work, done)

    def _disconnect(self):
        if self.conn:
            try:
                self.conn.close()
            except Exception:
                pass
        self.conn = None
        self.tables.clear()
        self.current = None
        self.table_list.clear()
        self.conn_status.setText("● 未连接")
        self.conn_status.setStyleSheet("color:#c0392b")
        self._show_hint(self.manual_form, "先在左侧选择一张表")
        self._show_hint(self.batch_form, "先在左侧选择一张表")

    def _refresh_tables(self):
        if not self._need_conn():
            return

        def work():
            with self.db_lock:
                return dbcore.list_tables(self.conn)

        def done(res, err):
            if err:
                QMessageBox.critical(self, "失败", str(err))
            else:
                self._fill_tables(res)
        self.run_bg(work, done)

    def _fill_tables(self, tables: list[str]):
        self.table_list.clear()
        self.table_list.addItems(tables)

    # ------------------------------------------------------------ 表选择 / 表单

    def _on_table_select(self, name: str):
        if not name or not self.conn:
            return

        def work():
            with self.db_lock:
                table = dbcore.describe_table(self.conn, name)
                dist = {c.name: dbcore.distinct_values(self.conn, name, c.name)
                        for c in table.columns if c.data_type == "text"}
                ids = dbcore.device_ids(self.conn)
            return table, dist, ids

        def done(res, err):
            if err:
                QMessageBox.critical(self, "读取表结构失败", str(err))
                return
            self.current, self.distinct, ids = res
            self.tables[self.current.name] = self.current
            self._build_manual_form()
            self._build_batch_form()
            for cb in (self.preset_device, self.preset_device2):
                cb.clear()
                cb.addItems(ids)
        self.run_bg(work, done)

    def _show_hint(self, layout: QGridLayout, text: str):
        while layout.count():
            item = layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        if text:
            hint = QLabel(text)
            hint.setStyleSheet("color:#888")
            layout.addWidget(hint, 0, 0)

    def _build_manual_form(self):
        table = self.current
        self._show_hint(self.manual_form, "")
        self.manual_rows = []
        for r, col in enumerate(table.columns):
            flags = (" PK" if col.is_pk else "") + ("" if col.nullable else " NOT NULL")
            self.manual_form.addWidget(
                QLabel(f"{col.name} : {col.short_type}{flags}"), r, 0)
            if col.data_type == "boolean":
                w = QComboBox()
                w.setEditable(True)
                w.addItems(["true", "false"])
                w.setCurrentText("")
            elif col.data_type == "text":
                w = QComboBox()
                w.setEditable(True)
                w.addItems(self.distinct.get(col.name, []))
                w.setCurrentText("")
            else:
                w = QLineEdit()
            w.setMinimumWidth(260)
            self.manual_form.addWidget(w, r, 1)
            row = {"col": col, "widget": w, "use_default": None}
            if col.has_default:
                cb = QCheckBox("用默认值")
                cb.setChecked(col.is_serial or col.name == "created_at")
                cb.toggled.connect(lambda checked, wd=w: wd.setEnabled(not checked))
                w.setEnabled(not cb.isChecked())
                self.manual_form.addWidget(cb, r, 2)
                row["use_default"] = cb
            self.manual_rows.append(row)

    def _build_batch_form(self):
        table = self.current
        self._show_hint(self.batch_form, "")
        self.batch_rows = {}
        for c, text in enumerate(("列", "策略", "参数")):
            lbl = QLabel(text)
            lbl.setStyleSheet("font-weight:bold")
            self.batch_form.addWidget(lbl, 0, c)
        for r, col in enumerate(table.columns, start=1):
            flags = (" PK" if col.is_pk else "") + ("" if col.nullable else " NOT NULL")
            self.batch_form.addWidget(
                QLabel(f"{col.name} : {col.short_type}{flags}"), r, 0)
            strategy, param = dbcore.default_strategy(col)
            sv = QComboBox()
            sv.addItems(dbcore.STRATEGIES)
            sv.setCurrentText(strategy)
            self.batch_form.addWidget(sv, r, 1)
            pv = QLineEdit(param)
            pv.setMinimumWidth(340)
            self.batch_form.addWidget(pv, r, 2)
            self.batch_rows[col.name] = (sv, pv)

    def _toggle_new_device(self, checked: bool):
        self.preset_device.setEnabled(not checked)
        for w in (self.preset_new_id, self.preset_name, self.preset_loc):
            w.setEnabled(checked)

    # ------------------------------------------------------------ 操作

    def _need_conn(self) -> bool:
        if not self.conn:
            QMessageBox.warning(self, "未连接", "请先连接数据库")
            return False
        return True

    def _widget_text(self, w) -> str:
        return w.currentText() if isinstance(w, QComboBox) else w.text()

    def _do_manual_insert(self):
        if not self._need_conn() or not self.current:
            return
        assignments = {}
        try:
            for row in self.manual_rows:
                col: dbcore.Column = row["col"]
                if row["use_default"] and row["use_default"].isChecked():
                    assignments[col.name] = dbcore.USE_DEFAULT
                    continue
                raw = self._widget_text(row["widget"]).strip()
                if raw == "":
                    if col.nullable:
                        assignments[col.name] = None
                    elif col.has_default:
                        assignments[col.name] = dbcore.USE_DEFAULT
                    else:
                        raise ValueError(f"列 {col.name} 不可为空")
                else:
                    assignments[col.name] = dbcore.parse_value(col, raw)
        except ValueError as e:
            QMessageBox.critical(self, "输入错误", str(e))
            return
        table_name = self.current.name

        def work():
            with self.db_lock:
                dbcore.insert_row(self.conn, table_name, assignments)

        def done(_res, err):
            if err:
                self.manual_result.setText(f"失败:{err}")
                self.manual_result.setStyleSheet("color:#c0392b")
            else:
                self.manual_result.setText(f"已插入 1 行到 {table_name}")
                self.manual_result.setStyleSheet("color:#27ae60")
        self.run_bg(work, done)

    def _do_batch(self):
        if not self._need_conn() or not self.current:
            return
        n = self.batch_n.value()
        table = self.current
        specs = {name: (sv.currentText(), pv.text())
                 for name, (sv, pv) in self.batch_rows.items()}
        self.cancel_event = threading.Event()
        cancel = self.cancel_event
        self.btn_batch.setEnabled(False)
        self.btn_batch_cancel.setEnabled(True)
        self.prog.setValue(0)
        self.prog_label.setText("0/?")

        def work():
            with self.db_lock:
                ctx = {}
                if any(s == "随机设备ID" for s, _ in specs.values()):
                    ctx["device_ids"] = dbcore.device_ids(self.conn)
                cols, rows_factory = dbcore.batch_plan(table, specs, ctx)
                return dbcore.insert_rows(
                    self.conn, table.name, cols, rows_factory(n), total=n,
                    progress=lambda d, t: self.bridge.progress.emit(d, t),
                    cancel=cancel)

        def done(res, err):
            self.btn_batch.setEnabled(True)
            self.btn_batch_cancel.setEnabled(False)
            if err:
                if isinstance(err, dbcore.Cancelled):
                    self.prog_label.setText("已取消并回滚")
                else:
                    self.prog_label.setText("失败,已回滚")
                    QMessageBox.critical(self, "批量插入失败", str(err))
            else:
                self.prog_label.setText(f"完成,共插入 {res} 行")
        self.run_bg(work, done)

    def _cancel_batch(self):
        if self.cancel_event:
            self.cancel_event.set()

    def _do_lux_preset(self):
        if not self._need_conn():
            return
        days, step = self.preset_days.value(), self.preset_step.value()
        new = self.preset_new.isChecked()
        device_id = (self.preset_new_id.text().strip() if new
                     else self.preset_device.currentText().strip())
        if not device_id:
            QMessageBox.critical(self, "参数错误", "请选择或填写设备 ID")
            return
        name, loc = self.preset_name.text().strip(), self.preset_loc.text().strip()
        self.cancel_event = threading.Event()
        cancel = self.cancel_event
        self.prog.setValue(0)
        self.prog_label.setText("0/?")

        def work():
            with self.db_lock:
                return dbcore.seed_lux_curve(
                    self.conn, device_id, days, step, new_device=new,
                    name=name, location=loc,
                    progress=lambda d, t: self.bridge.progress.emit(d, t),
                    cancel=cancel)

        def done(res, err):
            if err:
                if isinstance(err, dbcore.Cancelled):
                    self.prog_label.setText("已取消并回滚")
                else:
                    self.prog_label.setText("失败")
                    QMessageBox.critical(self, "生成失败", str(err))
            else:
                self.prog_label.setText(f"完成:{device_id} 回填 {res} 行光照数据")
        self.run_bg(work, done)

    def _do_online(self, online: bool):
        if not self._need_conn():
            return
        device_id = self.preset_device2.currentText().strip()
        if not device_id:
            QMessageBox.critical(self, "参数错误", "请选择设备")
            return

        def work():
            with self.db_lock:
                return dbcore.set_device_online(self.conn, device_id, online)

        def done(res, err):
            if err:
                QMessageBox.critical(self, "失败", str(err))
            else:
                self.preset_result.setText(str(res))
        self.run_bg(work, done)

    # ------------------------------------------------------------ 退出

    def closeEvent(self, event):
        if self.cancel_event:
            self.cancel_event.set()
        if self.conn:
            try:
                self.conn.close()
            except Exception:
                pass
        super().closeEvent(event)


def main():
    # WSLg 下走 wayland:缩放由合成器正确处理;xcb 平台缺系统库不可用。
    # 用户已显式设置 QT_QPA_PLATFORM 时尊重用户。
    if os.environ.get("WAYLAND_DISPLAY") and "QT_QPA_PLATFORM" not in os.environ:
        os.environ["QT_QPA_PLATFORM"] = "wayland"
    app = QApplication(sys.argv)
    win = MainWindow()
    win.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
