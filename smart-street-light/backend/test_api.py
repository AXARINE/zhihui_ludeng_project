# -*- coding: utf-8 -*-
"""后端接口冒烟测试：用标准库 urllib 以严格 UTF-8 验证全链路（可直接运行）。"""
import json
import urllib.request
import urllib.error

BASE = "http://127.0.0.1:8000"


def call(method, path, body=None):
    data = json.dumps(body, ensure_ascii=False).encode("utf-8") if body is not None else None
    req = urllib.request.Request(BASE + path, data=data, method=method,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read().decode("utf-8"))


def show(label, status, body):
    print(f"[{status}] {label}: {json.dumps(body, ensure_ascii=False)}")


if __name__ == "__main__":
    show("健康检查", *call("GET", "/api/health"))

    # 清理可能残留的账号
    _, users = call("GET", "/api/users")
    for u in users:
        if u["username"] in ("municipal", "admin"):
            show("删除旧账号", *call("DELETE", f"/api/users/{u['id']}"))

    show("新增市政人员(中文姓名)", *call("POST", "/api/users",
        {"username": "municipal", "password": "123456", "real_name": "张工", "role_id": 1}))
    show("新增路灯管理员(中文姓名)", *call("POST", "/api/users",
        {"username": "admin", "password": "123456", "real_name": "李工", "role_id": 2}))
    show("重复用户名(应409)", *call("POST", "/api/users",
        {"username": "municipal", "password": "123456", "role_id": 1}))
    show("账号列表", *call("GET", "/api/users"))

    show("上报光照", *call("POST", "/api/data/luminance",
        {"device_id": "lamp_001", "luminance": 245.5}))
    show("上报心跳(在线)", *call("POST", "/api/data/heartbeat",
        {"device_id": "lamp_001", "online_status": True}))
    show("上报告警(中文)", *call("POST", "/api/data/alarm",
        {"device_id": "lamp_001", "alarm_type": "offline", "message": "设备离线告警"}))

    show("设备列表", *call("GET", "/api/devices"))
    show("告警列表", *call("GET", "/api/alarms"))
    show("光照列表", *call("GET", "/api/luminance"))
