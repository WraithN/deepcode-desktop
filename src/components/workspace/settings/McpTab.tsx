import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Plug, Plus, Trash2 } from 'lucide-react';
import type { MCPServer } from './settings-utils';

interface McpTabProps {
  mcpServers: MCPServer[];
  onMcpServerChange: (servers: MCPServer[]) => void;
}

export default function McpTab({ mcpServers, onMcpServerChange }: McpTabProps) {
  const handleAddMcpServer = () => {
    onMcpServerChange([
      ...mcpServers,
      { id: Date.now().toString(), name: '', command: '', args: '', env: '', enabled: true },
    ]);
  };

  const handleRemoveMcpServer = (id: string) => {
    onMcpServerChange(mcpServers.filter((s) => s.id !== id));
  };

  const handleUpdateMcpServer = (id: string, field: keyof MCPServer, value: string | boolean) => {
    onMcpServerChange(mcpServers.map((s) => (s.id === id ? { ...s, [field]: value } : s)));
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Plug className="w-4 h-4 text-primary" />
          <span className="text-sm font-medium text-foreground">MCP 服务器配置</span>
        </div>
        <button
          type="button"
          onClick={handleAddMcpServer}
          className="flex items-center gap-1 px-2 py-1 text-[12px] rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
        >
          <Plus className="w-3 h-3" /> 添加
        </button>
      </div>
      <p className="text-[12px] text-muted-foreground leading-relaxed">Model Context Protocol (MCP) 允许智能体通过标准化接口与外部工具和数据源交互。</p>
      <div className="space-y-3 max-h-[400px] overflow-y-auto">
        {mcpServers.map((server) => (
          <div key={server.id} className="rounded border border-border bg-secondary/30 p-3 space-y-2">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={server.enabled}
                  onChange={(e) => handleUpdateMcpServer(server.id, 'enabled', e.target.checked)}
                  className="w-3.5 h-3.5 rounded border-border accent-primary"
                />
                <span className="text-xs font-medium text-foreground">{server.name || '未命名'}</span>
              </div>
              <button
                type="button"
                onClick={() => handleRemoveMcpServer(server.id)}
                className="text-muted-foreground hover:text-destructive transition-colors"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            </div>
            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <Label className="text-xs text-muted-foreground">名称</Label>
                <Input
                  value={server.name}
                  onChange={(e) => handleUpdateMcpServer(server.id, 'name', e.target.value)}
                  placeholder="server-name"
                  className="bg-secondary border-border text-xs h-7"
                />
              </div>
              <div className="space-y-1">
                <Label className="text-xs text-muted-foreground">命令</Label>
                <Input
                  value={server.command}
                  onChange={(e) => handleUpdateMcpServer(server.id, 'command', e.target.value)}
                  placeholder="npx, uvx, python..."
                  className="bg-secondary border-border text-xs h-7"
                />
              </div>
            </div>
            <div className="space-y-1">
              <Label className="text-xs text-muted-foreground">参数</Label>
              <Input
                value={server.args}
                onChange={(e) => handleUpdateMcpServer(server.id, 'args', e.target.value)}
                placeholder="-y @modelcontextprotocol/server-filesystem"
                className="bg-secondary border-border text-xs h-7"
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs text-muted-foreground">环境变量</Label>
              <Input
                value={server.env}
                onChange={(e) => handleUpdateMcpServer(server.id, 'env', e.target.value)}
                placeholder="KEY=VALUE;KEY2=VALUE2"
                className="bg-secondary border-border text-xs h-7"
              />
            </div>
          </div>
        ))}
        {mcpServers.length === 0 && (
          <div className="text-center py-8 text-xs text-muted-foreground">暂无MCP服务器，点击上方「添加」按钮创建</div>
        )}
      </div>
    </div>
  );
}
