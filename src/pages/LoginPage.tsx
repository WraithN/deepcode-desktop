import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/AuthContext';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import WindowTitleBar from '@/components/common/WindowTitleBar';
import { toast } from 'sonner';
import { Code2, Settings, Terminal } from 'lucide-react';

function getPostLoginPath(): string {
  try {
    const raw = localStorage.getItem('agent_instances');
    const agents = raw ? JSON.parse(raw) : [];
    return Array.isArray(agents) && agents.length > 0 ? '/workspace' : '/select-agent';
  } catch {
    return '/select-agent';
  }
}

export default function LoginPage() {
  console.log("[LoginPage.tsx] Rendering...");
  const [isLogin, setIsLogin] = useState(true);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [agreed, setAgreed] = useState(false);
  const [loading, setLoading] = useState(false);
  const [cloudUrlOpen, setCloudUrlOpen] = useState(false);
  const [cloudUrl, setCloudUrl] = useState(() => localStorage.getItem('cloud_url') || '');
  const { mockSignIn } = useAuth();
  const navigate = useNavigate();

  const handleSaveCloudUrl = () => {
    localStorage.setItem('cloud_url', cloudUrl.trim());
    toast.success('云端地址已保存');
    setCloudUrlOpen(false);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!isLogin && !agreed) {
      toast.error('请阅读并同意用户协议和隐私政策');
      return;
    }

    setLoading(true);
    // Mock登录：输入任何字符都能登录
    const finalUsername = username.trim() || 'guest';
    await mockSignIn(finalUsername);
    toast.success(isLogin ? '登录成功' : '注册成功');
    navigate(getPostLoginPath());
    setLoading(false);
  };

  return (
    <div className="min-h-screen flex flex-col bg-background">
      <WindowTitleBar title="DeepHarness Desktop">
        <div data-no-drag className="ml-auto mr-2 h-full flex items-center [-webkit-app-region:no-drag]">
          <button
            type="button"
            onPointerDown={(event) => event.stopPropagation()}
            onMouseDown={(event) => event.stopPropagation()}
            onClick={() => setCloudUrlOpen(true)}
            className="h-7 w-7 flex items-center justify-center rounded text-muted-foreground hover:text-foreground hover:bg-secondary/70 transition-colors [-webkit-app-region:no-drag]"
            aria-label="设置云端地址"
            title="设置云端地址"
          >
            <Settings className="w-4 h-4" />
          </button>
        </div>
      </WindowTitleBar>
      <div className="flex flex-1 items-center justify-center">
        <div className="w-full max-w-md p-8">
        {/* Logo区域 */}
        <div className="flex flex-col items-center mb-8">
          <div className="w-16 h-16 rounded-lg bg-primary/10 flex items-center justify-center mb-4">
            <Terminal className="w-8 h-8 text-primary" />
          </div>
          <h1 className="text-2xl font-bold text-foreground">DeepHarness</h1>
          <p className="text-sm text-muted-foreground mt-1">智能编码助手</p>
        </div>

        {/* 表单卡片 */}
        <div className="bg-card border border-border rounded-lg p-6">
          <div className="flex mb-6 bg-secondary rounded-md p-1">
            <button
              type="button"
              onClick={() => setIsLogin(true)}
              className={`flex-1 py-2 text-sm font-medium rounded transition-colors ${
                isLogin ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              登录
            </button>
            <button
              type="button"
              onClick={() => setIsLogin(false)}
              className={`flex-1 py-2 text-sm font-medium rounded transition-colors ${
                !isLogin ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              注册
            </button>
          </div>

          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="username" className="text-sm font-normal">用户名</Label>
              <div className="relative">
                <Code2 className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                <Input
                  id="username"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder="请输入用户名"
                  className="pl-10 bg-secondary border-border"
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="password" className="text-sm font-normal">密码</Label>
              <Input
                id="password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="请输入密码"
                className="bg-secondary border-border"
              />
            </div>

            {!isLogin && (
              <div className="flex items-start gap-2">
                <Checkbox
                  id="agree"
                  checked={agreed}
                  onCheckedChange={(v) => setAgreed(v === true)}
                  className="mt-0.5"
                />
                <label htmlFor="agree" className="text-xs text-muted-foreground leading-relaxed cursor-pointer">
                  我已阅读并同意
                  <span className="text-primary mx-1 cursor-pointer hover:underline">用户协议</span>
                  和
                  <span className="text-primary mx-1 cursor-pointer hover:underline">隐私政策</span>
                </label>
              </div>
            )}

            <Button
              type="submit"
              disabled={loading}
              className="w-full bg-primary text-primary-foreground hover:bg-primary/90"
            >
              {loading ? '处理中...' : isLogin ? '登录' : '注册'}
            </Button>
          </form>

          <div className="relative my-4">
            <div className="absolute inset-0 flex items-center">
              <span className="w-full border-t border-border" />
            </div>
            <div className="relative flex justify-center text-xs">
              <span className="bg-card px-2 text-muted-foreground">或</span>
            </div>
          </div>

          <Button
            type="button"
            variant="outline"
            disabled={loading}
            onClick={async () => {
              setLoading(true);
              await mockSignIn('guest');
              toast.success('访客登录成功');
              navigate(getPostLoginPath());
              setLoading(false);
            }}
            className="w-full"
          >
            访客登录
          </Button>
        </div>

          <p className="text-center text-xs text-muted-foreground mt-6">
            © 2026 DeepHarness. All rights reserved.
          </p>
        </div>
      </div>

      <Dialog open={cloudUrlOpen} onOpenChange={setCloudUrlOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>云端地址设置</DialogTitle>
            <DialogDescription>请输入 DeepHarness 云端服务 URL 地址。</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="cloud-url" className="text-sm font-normal">云端 URL</Label>
            <Input
              id="cloud-url"
              value={cloudUrl}
              onChange={(e) => setCloudUrl(e.target.value)}
              placeholder="https://example.com"
              className="bg-secondary border-border"
            />
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setCloudUrlOpen(false)}>取消</Button>
            <Button type="button" onClick={handleSaveCloudUrl}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}