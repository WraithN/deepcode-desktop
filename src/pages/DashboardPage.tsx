import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  AreaChart,
  Area,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { cn } from '@/lib/utils';
import WindowTitleBar from '@/components/common/WindowTitleBar';
import {
  Bot,
  MessageSquare,
  Activity,
  CheckCircle2,
  FileEdit,
  Users,
  TrendingUp,
  ArrowUpRight,
  ArrowDownRight,
  RefreshCw,
  ExternalLink,
  Clock,
  Server,
  Zap,
  BarChart3,
} from 'lucide-react';

const statsCards = [
  {
    title: '智能体实例',
    value: '12',
    change: '+3',
    trend: 'up' as const,
    icon: Bot,
    desc: '5 个活跃中',
  },
  {
    title: '总会话数',
    value: '1,284',
    change: '+12.5%',
    trend: 'up' as const,
    icon: MessageSquare,
    desc: '本月新增 156',
  },
  {
    title: '今日消息',
    value: '3,842',
    change: '+8.2%',
    trend: 'up' as const,
    icon: Activity,
    desc: '峰值 420条/时',
  },
  {
    title: '完成任务',
    value: '856',
    change: '+23.1%',
    trend: 'up' as const,
    icon: CheckCircle2,
    desc: '完成率 92.3%',
  },
  {
    title: '文件变更',
    value: '2,431',
    change: '-2.4%',
    trend: 'down' as const,
    icon: FileEdit,
    desc: '新增 1,892 修改 539',
  },
  {
    title: '活跃用户',
    value: '48',
    change: '+6',
    trend: 'up' as const,
    icon: Users,
    desc: '今日登录',
  },
];

const weeklyTrendData = [
  { day: '周一', 会话: 320, 消息: 1280 },
  { day: '周二', 会话: 480, 消息: 1920 },
  { day: '周三', 会话: 410, 消息: 1640 },
  { day: '周四', 会话: 560, 消息: 2240 },
  { day: '周五', 会话: 720, 消息: 2880 },
  { day: '周六', 会话: 390, 消息: 1560 },
  { day: '周日', 会话: 280, 消息: 1120 },
];

const agentBarData = [
  { name: 'OpenCode', 会话: 48, 活跃: 12 },
  { name: 'Claude Code', 会话: 36, 活跃: 8 },
  { name: 'Cursor', 会话: 24, 活跃: 5 },
  { name: 'Codex', 会话: 18, 活跃: 3 },
  { name: '自定义', 会话: 12, 活跃: 2 },
];

const recentActivities = [
  { time: '14:32:15', user: 'admin', action: '创建会话', target: 'OpenCode - 项目重构', status: 'success' },
  { time: '14:28:03', user: 'developer1', action: '发送消息', target: 'Claude Code - Bug修复', status: 'success' },
  { time: '14:22:47', user: 'admin', action: '完成任务', target: '实现用户认证模块', status: 'success' },
  { time: '14:15:30', user: 'tester', action: '查看文件', target: 'src/services/auth.ts', status: 'info' },
  { time: '14:08:12', user: 'developer1', action: '停止实例', target: 'Codex - 文档生成', status: 'warning' },
  { time: '13:55:44', user: 'admin', action: '修改配置', target: 'MCP 服务器配置', status: 'info' },
  { time: '13:42:18', user: 'developer2', action: '创建会话', target: 'Cursor - 单元测试', status: 'success' },
  { time: '13:30:05', user: 'tester', action: '文件变更', target: 'src/utils/helpers.ts', status: 'success' },
];

const statusBadge = (status: string) => {
  const map: Record<string, { label: string; cls: string }> = {
    success: {
      label: '成功',
      cls: 'bg-green-400/10 text-green-400 border-green-400/20',
    },
    warning: {
      label: '警告',
      cls: 'bg-yellow-400/10 text-yellow-400 border-yellow-400/20',
    },
    info: {
      label: '信息',
      cls: 'bg-blue-400/10 text-blue-400 border-blue-400/20',
    },
  };
  const s = map[status] || map.info;
  return (
    <Badge
      variant="outline"
      className={cn('text-[10px] px-1.5 py-0 h-4 font-normal', s.cls)}
    >
      {s.label}
    </Badge>
  );
};

export default function DashboardPage() {
  const navigate = useNavigate();
  const [timeRange, setTimeRange] = useState('today');

  return (
    <div className="min-h-screen flex flex-col bg-background select-none">
      <WindowTitleBar title="dh" />
      <div className="flex-1 overflow-auto p-6">
        <div className="max-w-[1400px] mx-auto space-y-6">

          {/* Header */}
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-[18px] font-semibold text-foreground">数据大盘</h1>
              <p className="text-sm text-muted-foreground mt-0.5">系统运行状态与数据分析概览</p>
            </div>
            <div className="flex items-center gap-2">
              <Tabs value={timeRange} onValueChange={setTimeRange} className="h-8">
                <TabsList className="h-8">
                  <TabsTrigger value="today" className="text-xs px-3 py-1">今日</TabsTrigger>
                  <TabsTrigger value="week" className="text-xs px-3 py-1">本周</TabsTrigger>
                  <TabsTrigger value="month" className="text-xs px-3 py-1">本月</TabsTrigger>
                </TabsList>
              </Tabs>
              <Button variant="outline" size="sm" className="h-8 gap-1.5">
                <RefreshCw className="w-3.5 h-3.5" />
                刷新
              </Button>
              <Button variant="default" size="sm" className="h-8 gap-1.5" onClick={() => navigate('/workspace')}>
                <ExternalLink className="w-3.5 h-3.5" />
                进入工作区
              </Button>
            </div>
          </div>

          {/* Stats Cards */}
          <div className="grid grid-cols-6 gap-3">
            {statsCards.map((card) => {
              const Icon = card.icon;
              return (
                <Card key={card.title} className="bg-card border-border shadow-sm hover:border-primary/20 transition-colors">
                  <CardContent className="p-4">
                    <div className="flex items-start justify-between mb-2">
                      <span className="text-xs text-muted-foreground">{card.title}</span>
                      <div className="w-7 h-7 rounded-md bg-primary/10 flex items-center justify-center shrink-0">
                        <Icon className="w-3.5 h-3.5 text-primary" />
                      </div>
                    </div>
                    <div className="text-xl font-semibold text-foreground leading-none mb-1.5">{card.value}</div>
                    <div className="flex items-center gap-1.5">
                      <span
                        className={cn(
                          'inline-flex items-center gap-0.5 text-xs font-medium',
                          card.trend === 'up' ? 'text-green-400' : 'text-red-400',
                        )}
                      >
                        {card.trend === 'up' ? (
                          <ArrowUpRight className="w-3 h-3" />
                        ) : (
                          <ArrowDownRight className="w-3 h-3" />
                        )}
                        {card.change}
                      </span>
                      <span className="text-xs text-muted-foreground">{card.desc}</span>
                    </div>
                  </CardContent>
                </Card>
              );
            })}
          </div>

          {/* Charts Row */}
          <div className="grid grid-cols-5 gap-3">
            {/* Weekly Trend - Area Chart */}
            <Card className="col-span-3 bg-card border-border shadow-sm">
              <CardHeader className="p-4 pb-0">
                <CardTitle className="text-sm font-medium flex items-center gap-2">
                  <TrendingUp className="w-4 h-4 text-primary" />
                  会话与消息趋势
                </CardTitle>
              </CardHeader>
              <CardContent className="p-4">
                <div className="h-52">
                  <ResponsiveContainer width="100%" height="100%">
                    <AreaChart data={weeklyTrendData} margin={{ top: 4, right: 4, bottom: 0, left: -20 }}>
                      <defs>
                        <linearGradient id="trendSessions" x1="0" y1="0" x2="0" y2="1">
                          <stop offset="5%" stopColor="hsl(var(--primary))" stopOpacity={0.3} />
                          <stop offset="95%" stopColor="hsl(var(--primary))" stopOpacity={0} />
                        </linearGradient>
                        <linearGradient id="trendMessages" x1="0" y1="0" x2="0" y2="1">
                          <stop offset="5%" stopColor="hsl(var(--primary))" stopOpacity={0.15} />
                          <stop offset="95%" stopColor="hsl(var(--primary))" stopOpacity={0} />
                        </linearGradient>
                      </defs>
                      <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" vertical={false} />
                      <XAxis dataKey="day" tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }} axisLine={false} tickLine={false} />
                      <YAxis tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }} axisLine={false} tickLine={false} />
                      <Tooltip
                        contentStyle={{
                          backgroundColor: 'hsl(var(--background))',
                          border: '1px solid hsl(var(--border))',
                          borderRadius: '4px',
                          fontSize: '12px',
                          boxShadow: 'none',
                        }}
                        labelStyle={{ color: 'hsl(var(--foreground))', fontWeight: 500 }}
                      />
                      <Area type="monotone" dataKey="会话" stroke="hsl(var(--primary))" strokeWidth={2} fill="url(#trendSessions)" dot={false} activeDot={{ r: 3, fill: 'hsl(var(--primary))' }} />
                      <Area type="monotone" dataKey="消息" stroke="hsl(var(--primary))" strokeWidth={1.5} strokeDasharray="4 3" fill="url(#trendMessages)" dot={false} activeDot={{ r: 3 }} />
                    </AreaChart>
                  </ResponsiveContainer>
                </div>
              </CardContent>
            </Card>

            {/* Agent Bar Chart */}
            <Card className="col-span-2 bg-card border-border shadow-sm">
              <CardHeader className="p-4 pb-0">
                <CardTitle className="text-sm font-medium flex items-center gap-2">
                  <BarChart3 className="w-4 h-4 text-primary" />
                  智能体会话分布
                </CardTitle>
              </CardHeader>
              <CardContent className="p-4">
                <div className="h-52">
                  <ResponsiveContainer width="100%" height="100%">
                    <BarChart data={agentBarData} margin={{ top: 4, right: 4, bottom: 0, left: -20 }} barCategoryGap="20%">
                      <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" vertical={false} />
                      <XAxis dataKey="name" tick={{ fontSize: 10, fill: 'hsl(var(--muted-foreground))' }} axisLine={false} tickLine={false} />
                      <YAxis tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }} axisLine={false} tickLine={false} />
                      <Tooltip
                        contentStyle={{
                          backgroundColor: 'hsl(var(--background))',
                          border: '1px solid hsl(var(--border))',
                          borderRadius: '4px',
                          fontSize: '12px',
                          boxShadow: 'none',
                        }}
                        labelStyle={{ color: 'hsl(var(--foreground))', fontWeight: 500 }}
                      />
                      <Bar dataKey="会话" fill="hsl(var(--primary))" radius={[2, 2, 0, 0]} maxBarSize={32} />
                      <Bar dataKey="活跃" fill="hsl(var(--primary))" fillOpacity={0.3} radius={[2, 2, 0, 0]} maxBarSize={32} />
                    </BarChart>
                  </ResponsiveContainer>
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Bottom Row */}
          <div className="grid grid-cols-5 gap-3">
            {/* Recent Activity */}
            <Card className="col-span-3 bg-card border-border shadow-sm">
              <CardHeader className="p-4 pb-0">
                <CardTitle className="text-sm font-medium flex items-center gap-2">
                  <Clock className="w-4 h-4 text-primary" />
                  最近活动
                </CardTitle>
              </CardHeader>
              <CardContent className="p-0">
                <Table>
                  <TableHeader>
                    <TableRow className="border-border">
                      <TableHead className="h-8 px-4 text-xs font-medium">时间</TableHead>
                      <TableHead className="h-8 px-4 text-xs font-medium">用户</TableHead>
                      <TableHead className="h-8 px-4 text-xs font-medium">操作</TableHead>
                      <TableHead className="h-8 px-4 text-xs font-medium">目标</TableHead>
                      <TableHead className="h-8 px-4 text-xs font-medium text-right">状态</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {recentActivities.map((row, i) => (
                      <TableRow key={i} className="border-border hover:bg-muted/30">
                        <TableCell className="px-4 py-2.5 text-xs text-muted-foreground font-mono">{row.time}</TableCell>
                        <TableCell className="px-4 py-2.5 text-xs text-foreground">{row.user}</TableCell>
                        <TableCell className="px-4 py-2.5 text-xs text-foreground">{row.action}</TableCell>
                        <TableCell className="px-4 py-2.5 text-xs text-muted-foreground max-w-[200px] truncate">{row.target}</TableCell>
                        <TableCell className="px-4 py-2.5 text-right">{statusBadge(row.status)}</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>

            {/* System Status */}
            <Card className="col-span-2 bg-card border-border shadow-sm">
              <CardHeader className="p-4 pb-2">
                <CardTitle className="text-sm font-medium flex items-center gap-2">
                  <Server className="w-4 h-4 text-primary" />
                  系统状态
                </CardTitle>
              </CardHeader>
              <CardContent className="p-4 pt-2">
                <div className="space-y-1">
                  {[
                    { label: '系统运行时间', value: '72h 34m', icon: Clock },
                    { label: '活跃 WebSocket', value: '8 连接', icon: Zap },
                    { label: 'Token 消耗(今日)', value: '1.2M', icon: Activity },
                    { label: '数据库大小', value: '24.6 MB', icon: Server },
                  ].map((item) => {
                    const Icon = item.icon;
                    return (
                      <div key={item.label} className="flex items-center justify-between py-2.5 border-b border-border last:border-b-0">
                        <div className="flex items-center gap-2">
                          <Icon className="w-3.5 h-3.5 text-muted-foreground" />
                          <span className="text-xs text-muted-foreground">{item.label}</span>
                        </div>
                        <span className="text-xs text-foreground font-mono">{item.value}</span>
                      </div>
                    );
                  })}
                </div>

                <Separator className="my-3" />

                <div className="space-y-3">
                  <div>
                    <div className="flex items-center justify-between mb-1.5">
                      <span className="text-xs text-muted-foreground">平均响应时间</span>
                      <Badge
                        variant="outline"
                        className="text-[10px] px-1.5 py-0 h-4 font-normal bg-green-400/10 text-green-400 border-green-400/20"
                      >
                        良好
                      </Badge>
                    </div>
                    <Progress value={28} className="h-1.5" />
                  </div>
                  <div>
                    <div className="flex items-center justify-between mb-1.5">
                      <span className="text-xs text-muted-foreground">CPU 使用率</span>
                      <span className="text-xs text-foreground font-mono">45%</span>
                    </div>
                    <Progress value={45} className="h-1.5" />
                  </div>
                  <div>
                    <div className="flex items-center justify-between mb-1.5">
                      <span className="text-xs text-muted-foreground">内存使用率</span>
                      <span className="text-xs text-foreground font-mono">62%</span>
                    </div>
                    <Progress value={62} className="h-1.5" />
                  </div>
                </div>

                <Separator className="my-3" />

                <div className="flex items-center justify-between">
                  <span className="text-xs text-muted-foreground">待处理任务</span>
                  <Badge variant="default" className="text-[10px] px-2 py-0 h-4 font-normal">3</Badge>
                </div>

                <div className="mt-3 text-center">
                  <Button variant="ghost" size="sm" className="text-xs text-muted-foreground h-7 gap-1" onClick={() => navigate('/workspace')}>
                    查看详细报告
                    <ArrowUpRight className="w-3 h-3" />
                  </Button>
                </div>
              </CardContent>
            </Card>
          </div>
        </div>
      </div>
    </div>
  );
}
