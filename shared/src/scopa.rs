//! Motore di gioco della Scopa (2 giocatori), autoritativo lato server ma
//! condiviso con il frontend per calcolare le prese possibili.
//!
//! Regole implementate:
//! - mazzo da 40, 3 carte a testa, 4 in tavola (si ridistribuisce se ci sono 3+ Re in tavola);
//! - se in tavola c'è una carta dello stesso valore la presa è obbligatoria su quella
//!   (presa singola ha la precedenza sulla somma);
//! - altrimenti si può prendere una combinazione la cui somma vale quanto la carta giocata;
//! - svuotare la tavola vale una scopa, tranne con l'ultima giocata della smazzata;
//! - a fine smazzata le carte in tavola vanno all'ultimo giocatore che ha preso;
//! - punti: carte, denari, settebello, primiera, scope. Si gioca fino a `target` punti.

use crate::cards::{Card, Suit};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const HAND_SIZE: usize = 3;
pub const TABLE_SIZE: usize = 4;
pub const DEFAULT_TARGET: u32 = 11;

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ScopaError {
    #[error("non è il tuo turno")]
    NotYourTurn,
    #[error("non hai questa carta in mano")]
    CardNotInHand,
    #[error("la presa indicata non è valida")]
    InvalidCapture,
    #[error("ci sono più prese possibili: scegline una")]
    AmbiguousCapture,
    #[error("la mano non è in corso")]
    NotPlaying,
    #[error("mazzo non valido")]
    InvalidDeck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Playing,
    /// Smazzata finita: si attende che i giocatori confermino la successiva.
    RoundOver,
    GameOver {
        winner: usize,
    },
}

/// Ultima giocata, utile per animazioni/log lato client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Move {
    pub player: usize,
    pub card: Card,
    pub captured: Vec<Card>,
    pub scopa: bool,
}

/// Dettaglio del punteggio di un giocatore in una smazzata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundScore {
    pub cards: u32,
    pub denari: u32,
    pub settebello: bool,
    pub primiera: u32,
    pub scope: u32,
    pub point_cards: bool,
    pub point_denari: bool,
    pub point_primiera: bool,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopaGame {
    pub num_players: usize,
    pub target: u32,
    pub deck: Vec<Card>,
    pub hands: Vec<Vec<Card>>,
    pub table: Vec<Card>,
    pub captured: Vec<Vec<Card>>,
    pub scope: Vec<u32>,
    pub scores: Vec<u32>,
    pub dealer: usize,
    pub turn: usize,
    pub round: u32,
    pub last_capturer: Option<usize>,
    pub last_move: Option<Move>,
    pub last_round: Option<Vec<RoundScore>>,
    pub phase: Phase,
}

impl ScopaGame {
    /// Crea una partita a 2 giocatori e distribuisce la prima smazzata.
    /// `deck` deve essere un mazzo da 40 già mescolato dal chiamante.
    pub fn new(target: u32, deck: Vec<Card>) -> Result<Self, ScopaError> {
        let n = 2;
        let mut g = ScopaGame {
            num_players: n,
            target: target.max(1),
            deck: Vec::new(),
            hands: vec![Vec::new(); n],
            table: Vec::new(),
            captured: vec![Vec::new(); n],
            scope: vec![0; n],
            scores: vec![0; n],
            dealer: n - 1,
            turn: 0,
            round: 0,
            last_capturer: None,
            last_move: None,
            last_round: None,
            phase: Phase::Playing,
        };
        g.start_round(deck)?;
        Ok(g)
    }

    /// Avvia una nuova smazzata con un mazzo mescolato.
    pub fn start_round(&mut self, mut deck: Vec<Card>) -> Result<(), ScopaError> {
        if matches!(self.phase, Phase::GameOver { .. }) {
            return Err(ScopaError::NotPlaying);
        }
        validate_deck(&deck)?;
        if self.round > 0 {
            self.dealer = (self.dealer + 1) % self.num_players;
        }
        self.round += 1;
        for h in &mut self.hands {
            h.clear();
        }
        for c in &mut self.captured {
            c.clear();
        }
        self.scope.iter_mut().for_each(|s| *s = 0);
        self.last_capturer = None;
        self.last_move = None;

        // Con 3 o più Re in tavola si ridistribuisce: ruotiamo il mazzo finché non va bene.
        for _ in 0..deck.len() {
            let kings = deck[..TABLE_SIZE].iter().filter(|c| c.rank == 10).count();
            if kings < 3 {
                break;
            }
            deck.rotate_left(1);
        }
        self.table = deck.drain(..TABLE_SIZE).collect();
        self.deck = deck;
        self.deal_hands();
        self.turn = (self.dealer + 1) % self.num_players;
        self.phase = Phase::Playing;
        Ok(())
    }

    fn deal_hands(&mut self) {
        for _ in 0..HAND_SIZE {
            for p in 0..self.num_players {
                let idx = (self.dealer + 1 + p) % self.num_players;
                if let Some(c) = self.deck.pop() {
                    self.hands[idx].push(c);
                }
            }
        }
    }

    /// Gioca una carta. `capture` indica le carte da prendere; se `None` e la presa
    /// è univoca viene scelta automaticamente.
    pub fn play(
        &mut self,
        player: usize,
        card: Card,
        capture: Option<Vec<Card>>,
    ) -> Result<Move, ScopaError> {
        if self.phase != Phase::Playing {
            return Err(ScopaError::NotPlaying);
        }
        if player != self.turn {
            return Err(ScopaError::NotYourTurn);
        }
        let pos = self.hands[player]
            .iter()
            .position(|&c| c == card)
            .ok_or(ScopaError::CardNotInHand)?;

        let options = capture_options(card, &self.table);
        let chosen: Vec<Card> = match (options.len(), capture) {
            (0, None) => Vec::new(),
            (0, Some(c)) if c.is_empty() => Vec::new(),
            (0, Some(_)) => return Err(ScopaError::InvalidCapture),
            (1, None) => options[0].clone(),
            (_, None) => return Err(ScopaError::AmbiguousCapture),
            (_, Some(c)) => {
                let mut wanted = c;
                wanted.sort();
                options
                    .into_iter()
                    .find(|o| {
                        let mut o = o.clone();
                        o.sort();
                        o == wanted
                    })
                    .ok_or(ScopaError::InvalidCapture)?
            }
        };

        self.hands[player].remove(pos);
        let mut scopa = false;
        if chosen.is_empty() {
            self.table.push(card);
        } else {
            self.table.retain(|c| !chosen.contains(c));
            self.captured[player].push(card);
            self.captured[player].extend(chosen.iter().copied());
            self.last_capturer = Some(player);
            let last_play = self.deck.is_empty() && self.hands.iter().all(|h| h.is_empty());
            if self.table.is_empty() && !last_play {
                scopa = true;
                self.scope[player] += 1;
            }
        }

        let mv = Move {
            player,
            card,
            captured: chosen,
            scopa,
        };
        self.last_move = Some(mv.clone());
        self.turn = (self.turn + 1) % self.num_players;

        if self.hands.iter().all(|h| h.is_empty()) {
            if self.deck.is_empty() {
                self.finish_round();
            } else {
                self.deal_hands();
            }
        }
        Ok(mv)
    }

    fn finish_round(&mut self) {
        if let Some(p) = self.last_capturer {
            let rest: Vec<Card> = self.table.drain(..).collect();
            self.captured[p].extend(rest);
        }
        let round = score_round(&self.captured, &self.scope);
        for (s, r) in self.scores.iter_mut().zip(&round) {
            *s += r.total;
        }
        self.last_round = Some(round);

        let max = *self.scores.iter().max().unwrap_or(&0);
        let leaders: Vec<usize> = (0..self.num_players)
            .filter(|&i| self.scores[i] == max)
            .collect();
        self.phase = if max >= self.target && leaders.len() == 1 {
            Phase::GameOver { winner: leaders[0] }
        } else {
            Phase::RoundOver
        };
    }

    /// Abbandono: l'altro giocatore vince a tavolino.
    pub fn forfeit(&mut self, player: usize) -> usize {
        let winner = (player + 1) % self.num_players;
        self.phase = Phase::GameOver { winner };
        winner
    }

    pub fn winner(&self) -> Option<usize> {
        match self.phase {
            Phase::GameOver { winner } => Some(winner),
            _ => None,
        }
    }
}

fn validate_deck(deck: &[Card]) -> Result<(), ScopaError> {
    let mut sorted = deck.to_vec();
    sorted.sort();
    let mut full = crate::cards::full_deck();
    full.sort();
    if sorted == full {
        Ok(())
    } else {
        Err(ScopaError::InvalidDeck)
    }
}

/// Tutte le prese possibili giocando `card` sulla `table`.
/// Se esistono carte di pari valore, le uniche prese ammesse sono quelle singole.
pub fn capture_options(card: Card, table: &[Card]) -> Vec<Vec<Card>> {
    let singles: Vec<Vec<Card>> = table
        .iter()
        .filter(|c| c.rank == card.rank)
        .map(|&c| vec![c])
        .collect();
    if !singles.is_empty() {
        return singles;
    }
    let target = card.rank as u32;
    let n = table.len();
    let mut out = Vec::new();
    // La tavola ha al massimo qualche decina di carte; nella pratica <= 10.
    for mask in 1u32..(1u32 << n.min(20)) {
        if mask.count_ones() < 2 {
            continue;
        }
        let sum: u32 = (0..n)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| table[i].rank as u32)
            .sum();
        if sum == target {
            out.push(
                (0..n)
                    .filter(|i| mask & (1 << i) != 0)
                    .map(|i| table[i])
                    .collect(),
            );
        }
    }
    out
}

fn primiera_of(cards: &[Card]) -> u32 {
    let mut total = 0;
    for suit in Suit::ALL {
        match cards
            .iter()
            .filter(|c| c.suit == suit)
            .map(|c| c.primiera_value())
            .max()
        {
            Some(v) => total += v,
            None => return 0,
        }
    }
    total
}

/// Assegna il punto a chi ha strettamente il valore più alto (nessuno in caso di parità).
fn unique_max(values: &[u32]) -> Option<usize> {
    let max = *values.iter().max()?;
    if max == 0 {
        return None;
    }
    let mut it = values.iter().enumerate().filter(|(_, &v)| v == max);
    let first = it.next()?.0;
    if it.next().is_some() {
        None
    } else {
        Some(first)
    }
}

pub fn score_round(captured: &[Vec<Card>], scope: &[u32]) -> Vec<RoundScore> {
    let mut res: Vec<RoundScore> = captured
        .iter()
        .zip(scope)
        .map(|(cards, &sc)| RoundScore {
            cards: cards.len() as u32,
            denari: cards.iter().filter(|c| c.suit == Suit::Denari).count() as u32,
            settebello: cards.iter().any(|c| c.is_settebello()),
            primiera: primiera_of(cards),
            scope: sc,
            ..Default::default()
        })
        .collect();

    let cards: Vec<u32> = res.iter().map(|r| r.cards).collect();
    let denari: Vec<u32> = res.iter().map(|r| r.denari).collect();
    let prim: Vec<u32> = res.iter().map(|r| r.primiera).collect();
    if let Some(i) = unique_max(&cards) {
        res[i].point_cards = true;
    }
    if let Some(i) = unique_max(&denari) {
        res[i].point_denari = true;
    }
    if let Some(i) = unique_max(&prim) {
        res[i].point_primiera = true;
    }
    for r in &mut res {
        r.total = r.scope
            + r.point_cards as u32
            + r.point_denari as u32
            + r.point_primiera as u32
            + r.settebello as u32;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::full_deck;

    fn c(suit: Suit, rank: u8) -> Card {
        Card::new(suit, rank)
    }

    #[test]
    fn single_capture_has_priority_over_sum() {
        let table = vec![c(Suit::Coppe, 3), c(Suit::Spade, 2), c(Suit::Bastoni, 5)];
        let opts = capture_options(c(Suit::Denari, 5), &table);
        assert_eq!(opts, vec![vec![c(Suit::Bastoni, 5)]]);
    }

    #[test]
    fn sum_capture_options() {
        let table = vec![
            c(Suit::Coppe, 3),
            c(Suit::Spade, 2),
            c(Suit::Bastoni, 4),
            c(Suit::Denari, 1),
        ];
        let opts = capture_options(c(Suit::Denari, 5), &table);
        // 3+2, 4+1
        assert_eq!(opts.len(), 2);
        assert!(capture_options(c(Suit::Denari, 10), &[c(Suit::Coppe, 1)]).is_empty());
    }

    #[test]
    fn full_game_plays_to_completion() {
        // Il primo giocatore gioca sempre la prima carta e sceglie la prima presa.
        let mut g = ScopaGame::new(11, full_deck()).unwrap();
        let mut guard = 0;
        while g.winner().is_none() {
            guard += 1;
            assert!(guard < 10_000);
            if g.phase == Phase::RoundOver {
                let total: u32 = g.captured.iter().map(|c| c.len() as u32).sum();
                assert_eq!(total, 40);
                let mut d = full_deck();
                d.rotate_left(guard % 40);
                g.start_round(d).unwrap();
                continue;
            }
            let p = g.turn;
            let card = g.hands[p][0];
            let cap = capture_options(card, &g.table).into_iter().next();
            g.play(p, card, cap).unwrap();
        }
        let w = g.winner().unwrap();
        assert!(g.scores[w] >= 11);
    }

    #[test]
    fn rejects_out_of_turn_and_ambiguous() {
        let mut g = ScopaGame::new(11, full_deck()).unwrap();
        let other = (g.turn + 1) % 2;
        let card = g.hands[other][0];
        assert_eq!(g.play(other, card, None), Err(ScopaError::NotYourTurn));

        g.table = vec![
            c(Suit::Coppe, 3),
            c(Suit::Spade, 2),
            c(Suit::Bastoni, 4),
            c(Suit::Denari, 1),
        ];
        let p = g.turn;
        g.hands[p] = vec![c(Suit::Denari, 5)];
        assert_eq!(
            g.play(p, c(Suit::Denari, 5), None),
            Err(ScopaError::AmbiguousCapture)
        );
        let mv = g
            .play(
                p,
                c(Suit::Denari, 5),
                Some(vec![c(Suit::Denari, 1), c(Suit::Bastoni, 4)]),
            )
            .unwrap();
        assert_eq!(mv.captured.len(), 2);
        assert_eq!(g.table.len(), 2);
    }

    #[test]
    fn scopa_is_counted() {
        let mut g = ScopaGame::new(11, full_deck()).unwrap();
        let p = g.turn;
        g.table = vec![c(Suit::Coppe, 3), c(Suit::Spade, 4)];
        g.hands[p] = vec![c(Suit::Denari, 7), c(Suit::Denari, 1)];
        let mv = g.play(p, c(Suit::Denari, 7), None).unwrap();
        assert!(mv.scopa);
        assert_eq!(g.scope[p], 1);
    }

    #[test]
    fn scoring_points() {
        let deck = full_deck();
        let (a, b): (Vec<Card>, Vec<Card>) = deck
            .iter()
            .partition(|c| c.suit == Suit::Denari || c.rank == 7);
        let r = score_round(&[a, b], &[2, 0]);
        assert!(r[0].settebello && r[0].point_denari && r[0].point_primiera);
        assert!(r[1].point_cards);
        assert_eq!(r[0].total, 2 + 3);
        assert_eq!(r[1].total, 1);
    }
}
