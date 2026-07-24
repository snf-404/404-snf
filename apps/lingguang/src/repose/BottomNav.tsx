import { Activity, Bluetooth, History, type LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { TranslationKey } from '@/i18n/resources'

export type Tab = 'status' | 'record' | 'device'

interface BottomNavProps {
  activeTab: Tab
  onTabChange: (tab: Tab) => void
}

const tabs: { id: Tab; labelKey: TranslationKey; icon: LucideIcon }[] = [
  { id: 'status', labelKey: 'nav.status', icon: Activity },
  { id: 'record', labelKey: 'nav.records', icon: History },
  { id: 'device', labelKey: 'nav.device', icon: Bluetooth },
]

function isTab(value: string): value is Tab {
  return value === 'status' || value === 'record' || value === 'device'
}

export function BottomNav({ activeTab, onTabChange }: BottomNavProps) {
  const { t } = useTranslation('translation')

  return (
    <nav className="repose-bottom-nav" aria-label={t('nav.ariaLabel')}>
      <Tabs
        value={activeTab}
        className="h-full"
        onValueChange={(value) => {
          if (isTab(value)) onTabChange(value)
        }}
      >
        <TabsList className="repose-tab-list" aria-label={t('nav.ariaLabel')}>
          {tabs.map((tab) => {
            const Icon = tab.icon
            return (
              <TabsTrigger
                key={tab.id}
                value={tab.id}
                data-testid={`nav-${tab.id}`}
                className="repose-tab-trigger"
              >
                <Icon size={25} strokeWidth={1.7} />
                <span>{t(tab.labelKey)}</span>
              </TabsTrigger>
            )
          })}
        </TabsList>
      </Tabs>
    </nav>
  )
}
