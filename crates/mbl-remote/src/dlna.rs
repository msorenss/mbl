//! DLNA / UPnP Media Server (M7.9):
//! SSDP XML descriptions, ContentDirectory DIDL-Lite browse trees, and renderer profiles.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowseFlag {
    BrowseMetadata,
    BrowseDirectChildren,
}

impl std::str::FromStr for BrowseFlag {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "BrowseMetadata" => Ok(Self::BrowseMetadata),
            _ => Ok(Self::BrowseDirectChildren),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidlItem {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub class: String, // "object.item.videoItem.movie", "object.item.audioItem.musicTrack", "object.container.storageFolder"
    pub res_url: Option<String>,
    pub protocol_info: Option<String>,
    pub size_bytes: Option<u64>,
    pub duration: Option<String>, // "H:MM:SS"
}

/// Generates UPnP root device description XML.
pub fn generate_device_description(_base_url: &str, server_name: &str, udn_uuid: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion>
    <major>1</major>
    <minor>0</minor>
  </specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType>
    <friendlyName>{}</friendlyName>
    <manufacturer>MBL Contributors</manufacturer>
    <manufacturerURL>https://github.com/msorenss/mbl</manufacturerURL>
    <modelDescription>MBL UPnP/DLNA Media Server</modelDescription>
    <modelName>MBL Media Server</modelName>
    <modelNumber>0.6.0</modelNumber>
    <UDN>uuid:{}</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:ContentDirectory:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:ContentDirectory</serviceId>
        <SCPDURL>/dlna/content_directory.xml</SCPDURL>
        <controlURL>/dlna/control</controlURL>
        <eventSubURL>/dlna/events</eventSubURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:ConnectionManager</serviceId>
        <SCPDURL>/dlna/connection_manager.xml</SCPDURL>
        <controlURL>/dlna/connection_control</controlURL>
        <eventSubURL>/dlna/connection_events</eventSubURL>
      </service>
    </serviceList>
  </device>
</root>"#,
        server_name, udn_uuid
    )
}

/// Generates ContentDirectory service SCPD XML definition.
pub fn generate_content_directory_scpd() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion>
    <major>1</major>
    <minor>0</minor>
  </specVersion>
  <actionList>
    <action>
      <name>GetSearchCapabilities</name>
      <argumentList>
        <argument>
          <name>SearchCaps</name>
          <direction>out</direction>
          <relatedStateVariable>SearchCapabilities</relatedStateVariable>
        </argument>
      </argumentList>
    </action>
    <action>
      <name>Browse</name>
      <argumentList>
        <argument>
          <name>ObjectID</name>
          <direction>in</direction>
          <relatedStateVariable>A_ARG_TYPE_ObjectID</relatedStateVariable>
        </argument>
        <argument>
          <name>BrowseFlag</name>
          <direction>in</direction>
          <relatedStateVariable>A_ARG_TYPE_BrowseFlag</relatedStateVariable>
        </argument>
        <argument>
          <name>Filter</name>
          <direction>in</direction>
          <relatedStateVariable>A_ARG_TYPE_Filter</relatedStateVariable>
        </argument>
        <argument>
          <name>StartingIndex</name>
          <direction>in</direction>
          <relatedStateVariable>A_ARG_TYPE_Index</relatedStateVariable>
        </argument>
        <argument>
          <name>RequestedCount</name>
          <direction>in</direction>
          <relatedStateVariable>A_ARG_TYPE_Count</relatedStateVariable>
        </argument>
        <argument>
          <name>SortCriteria</name>
          <direction>in</direction>
          <relatedStateVariable>A_ARG_TYPE_SortCriteria</relatedStateVariable>
        </argument>
        <argument>
          <name>Result</name>
          <direction>out</direction>
          <relatedStateVariable>A_ARG_TYPE_Result</relatedStateVariable>
        </argument>
        <argument>
          <name>NumberReturned</name>
          <direction>out</direction>
          <relatedStateVariable>A_ARG_TYPE_Count</relatedStateVariable>
        </argument>
        <argument>
          <name>TotalMatches</name>
          <direction>out</direction>
          <relatedStateVariable>A_ARG_TYPE_Count</relatedStateVariable>
        </argument>
        <argument>
          <name>UpdateID</name>
          <direction>out</direction>
          <relatedStateVariable>SystemUpdateID</relatedStateVariable>
        </argument>
      </argumentList>
    </action>
  </actionList>
  <serviceStateTable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_ObjectID</name>
      <dataType>string</dataType>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_Result</name>
      <dataType>string</dataType>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_SearchCriteria</name>
      <dataType>string</dataType>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_BrowseFlag</name>
      <dataType>string</dataType>
      <allowedValueList>
        <allowedValue>BrowseMetadata</allowedValue>
        <allowedValue>BrowseDirectChildren</allowedValue>
      </allowedValueList>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_Filter</name>
      <dataType>string</dataType>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_SortCriteria</name>
      <dataType>string</dataType>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_Index</name>
      <dataType>ui4</dataType>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_Count</name>
      <dataType>ui4</dataType>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>A_ARG_TYPE_UpdateID</name>
      <dataType>ui4</dataType>
    </stateVariable>
    <stateVariable sendEvents="yes">
      <name>SystemUpdateID</name>
      <dataType>ui4</dataType>
    </stateVariable>
    <stateVariable sendEvents="no">
      <name>SearchCapabilities</name>
      <dataType>string</dataType>
    </stateVariable>
  </serviceStateTable>
</scpd>"#
}

/// Generates DIDL-Lite XML string containing items and containers.
pub fn generate_didl_lite(items: &[DidlItem]) -> String {
    let mut out = String::from(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:dlna="urn:schemas-dlna-org:metadata-1-0/">"#,
    );

    for it in items {
        let is_container = it.class.starts_with("object.container");
        if is_container {
            out.push_str(&format!(
                r#"<container id="{}" parentID="{}" restricted="1"><dc:title>{}</dc:title><upnp:class>{}</upnp:class></container>"#,
                it.id, it.parent_id, escape_xml(&it.title), it.class
            ));
        } else {
            let res_tag = match (&it.res_url, &it.protocol_info) {
                (Some(url), Some(proto)) => {
                    let size_attr =
                        it.size_bytes.map(|s| format!(" size=\"{s}\"")).unwrap_or_default();
                    let dur_attr = it
                        .duration
                        .as_ref()
                        .map(|d| format!(" duration=\"{d}\""))
                        .unwrap_or_default();
                    format!(
                        "<res protocolInfo=\"{}\"{}{}>{}</res>",
                        proto, size_attr, dur_attr, url
                    )
                }
                _ => String::new(),
            };
            out.push_str(&format!(
                r#"<item id="{}" parentID="{}" restricted="1"><dc:title>{}</dc:title><upnp:class>{}</upnp:class>{}</item>"#,
                it.id, it.parent_id, escape_xml(&it.title), it.class, res_tag
            ));
        }
    }

    out.push_str("</DIDL-Lite>");
    out
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DlnaRendererProfile {
    Generic,
    SamsungTv,
    LgWebOs,
    SonyBravia,
}

impl DlnaRendererProfile {
    pub fn from_user_agent(ua: &str) -> Self {
        let l = ua.to_lowercase();
        if l.contains("samsung") || l.contains("tizen") {
            Self::SamsungTv
        } else if l.contains("webos") || l.contains("web0s") || l.contains("lg ") || l.contains("lge") {
            Self::LgWebOs
        } else if l.contains("sony") || l.contains("bravia") {
            Self::SonyBravia
        } else {
            Self::Generic
        }
    }

    /// Formats DLNA `protocolInfo` header with profile flags.
    pub fn dlna_protocol_info(&self, mime: &str, profile_name: &str) -> String {
        format!(
            "http-get:*:{mime}:DLNA.ORG_PN={profile_name};DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn didl_lite_generation() {
        let items = vec![
            DidlItem {
                id: "folder_1".into(),
                parent_id: "0".into(),
                title: "Movies".into(),
                class: "object.container.storageFolder".into(),
                res_url: None,
                protocol_info: None,
                size_bytes: None,
                duration: None,
            },
            DidlItem {
                id: "item_42".into(),
                parent_id: "folder_1".into(),
                title: "Inception".into(),
                class: "object.item.videoItem.movie".into(),
                res_url: Some("http://192.168.1.100:8420/api/v1/stream/direct/42".into()),
                protocol_info: Some("http-get:*:video/mp4:DLNA.ORG_PN=AVC_MP4_HP_HD_AAC".into()),
                size_bytes: Some(1_500_000_000),
                duration: Some("02:28:00".into()),
            },
        ];

        let didl = generate_didl_lite(&items);
        assert!(didl.contains("<container id=\"folder_1\""));
        assert!(didl.contains("<item id=\"item_42\""));
        assert!(didl.contains("Inception"));
        assert!(didl.contains("size=\"1500000000\""));
    }

    #[test]
    fn renderer_profile_detection() {
        assert_eq!(
            DlnaRendererProfile::from_user_agent("Mozilla/5.0 (SMART-TV; Linux; Tizen 6.0)"),
            DlnaRendererProfile::SamsungTv
        );
        assert_eq!(
            DlnaRendererProfile::from_user_agent("Mozilla/5.0 (Web0S; Linux/SmartTV)"),
            DlnaRendererProfile::LgWebOs
        );
    }
}
