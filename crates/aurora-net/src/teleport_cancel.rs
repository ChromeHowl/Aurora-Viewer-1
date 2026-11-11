//! TeleportCancel (LLAgent::teleportCancel, indra/newview/llagent.cpp,
//! originally LGPL 2.1): the agent and session in its Info block, sent
//! reliably to the region the teleport leaves.
use aurora_msg::msgs::TeleportCancel;
use uuid::Uuid;

/// LLAgent::teleportCancel sends it with sendReliableMessage.
pub(crate) const RELIABLE: bool = true;

pub(crate) fn packet(agent: Uuid, session: Uuid) -> TeleportCancel {
    let mut m = TeleportCancel::default();
    m.info.agent_id = agent;
    m.info.session_id = session;
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_msg::{Msg, MsgId, PacketFlags, build_packet, parse_packet};

    #[test]
    fn teleport_cancel_is_low_72_reliable_with_the_agent_and_session() {
        let agent = Uuid::from_u128(0x1111);
        let session = Uuid::from_u128(0x2222);
        let m = packet(agent, session);
        assert_eq!(TeleportCancel::ID, MsgId::Low(72));
        const { assert!(!TeleportCancel::ZEROCODED, "Unencoded in message_template.msg") };
        let mut wire = Vec::new();
        build_packet(&m, 7, RELIABLE, false, &[], &mut wire);
        assert_eq!(wire[0] & PacketFlags::RELIABLE, PacketFlags::RELIABLE);
        // header (flags, sequence, extra), then Low 72 and two UUIDs
        assert_eq!(&wire[6..10], &[0xFF, 0xFF, 0x00, 0x48]);
        assert_eq!(&wire[10..26], agent.as_bytes());
        assert_eq!(&wire[26..42], session.as_bytes());
        assert_eq!(wire.len(), 42);
        let parsed = parse_packet(&wire).expect("parses");
        assert_eq!(parsed.id, MsgId::Low(72));
        let back = TeleportCancel::decode(&parsed.body).expect("decodes");
        assert_eq!((back.info.agent_id, back.info.session_id), (agent, session));
    }
}
