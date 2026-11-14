# frozen_string_literal: true

require_relative 'saga_machines/version'

# Every value mirrors the core type of the same name, field for field. Every
# Data value validates at construction through the core, whether the gem
# built it or you did: TypeError for the wrong class, RangeError for a value
# the core does not accept.
module SagaMachines
  # The core's `SagaId`: the workflow's identity in the dedup namespace, an
  # Integer in 0..2**64-1 the caller owns. `SagaId.new(raw)` is
  # `SagaId::from_raw`.
  SagaId = Data.define(:raw) do
    # `SagaId::from_bytes`: a stable identity from an external name (FNV-1a
    # over the String's bytes, the same in every process).
    def self.from_bytes(name) = new(Saga.__send__(:native_from_bytes, name))

    def initialize(raw:)
      Saga.__send__(:native_saga_id, raw)
      super
    end
  end

  # The core's `AttemptId`: the handle of one try, issued once per saga and
  # never again. `raw` is its Integer form, the saga's scope in the top 16
  # bits above a sequence starting at 1. `AttemptId.new(raw)` is
  # `AttemptId::from_raw`: RangeError for a sequence of zero, which no saga
  # issues; any other Integer builds one, and the saga answers :stale to one
  # it never issued.
  AttemptId = Data.define(:raw) do
    def initialize(raw:)
      Saga.__send__(:native_scope_of, raw)
      super
    end

    # The scope of the saga that issued this attempt.
    def scope = Saga.__send__(:native_scope_of, raw)
  end

  # The core's `ActionKey`: which saga (a SagaId), which step, which kind
  # (:forward or :compensate). The same for every attempt at that work, before
  # and after a restart: the identity to deduplicate the effect by.
  ActionKey = Data.define(:saga, :step, :kind) do
    def initialize(saga:, step:, kind:)
      Saga.__send__(:native_key, saga, step, kind)
      super
    end
  end

  # The core's `Action`: one attempt at one piece of work, the stable
  # ActionKey plus the AttemptId issued for this try. Plain data: store it
  # before running the effect, rebuild it when the reply arrives.
  Action = Data.define(:key, :attempt) do
    def initialize(key:, attempt:)
      Saga.__send__(:native_action, key, attempt)
      super
    end

    def step = key.step
    def kind = key.kind
  end

  # The core's `Progress::Forward(step)` and `Progress::Compensating(step)`.
  # The other two variants are the symbols :complete and :aborted.
  stepped = proc do
    def initialize(step:)
      Saga.__send__(:native_step, step)
      super
    end
  end
  Forward = Data.define(:step, &stepped)
  Compensating = Data.define(:step, &stepped)

  # The core's `Settlement`: the last action resolved and the outcome
  # (:success or :failure) it was resolved with.
  Settlement = Data.define(:action, :outcome) do
    def initialize(action:, outcome:)
      Saga.__send__(:native_settlement, action, outcome)
      super
    end
  end

  # The core's `Snapshot`: everything a saga saves, in the core's field
  # order. `progress` is a Forward, a Compensating, :complete or :aborted;
  # `settled` a Settlement or nil. Built through `Snapshot::new`: RangeError
  # for an `issued` past 2**48-1, or a settlement of another saga, another
  # scope or an attempt beyond `issued`. What depends on the number of steps
  # is checked by `Saga.restored`.
  Snapshot = Data.define(:id, :scope, :progress, :issued, :settled) do
    def initialize(id:, scope:, progress:, issued:, settled:)
      Saga.__send__(:native_snapshot, id, scope, progress, issued, settled)
      super
    end
  end

end

# Native is required: no algorithm fallback. It resolves the Data values above
# once, so they must exist first.
require 'saga_machines_native/saga_machines_native'

module SagaMachines
  # Data values from the flat rows the native readers answer. Every value the
  # extension reads back (a SagaId, an Action, a Snapshot) it reads itself.
  module Wire
    module_function

    def action(row)
      saga, step, kind, attempt = row
      Action.new(ActionKey.new(SagaId.new(saga), step, kind), AttemptId.new(attempt))
    end

    def progress(name, step)
      case name
      when :forward then Forward.new(step)
      when :compensating then Compensating.new(step)
      else name
      end
    end

    def settled(row)
      row && Settlement.new(action(row.first(4)), row.last)
    end

    def snapshot(row)
      id, scope, progress, issued, settled = row
      Snapshot.new(SagaId.new(id), scope, progress(*progress), issued, settled(settled))
    end
  end
  private_constant :Wire

  # The values are final. `initialize` is where each validates, and a subclass
  # could override it, or `allocate` build an instance that never ran it:
  # subclassing raises TypeError (as subclassing Class does) and `allocate`
  # is gone. `new`, keywords and `with` still run the check. That stops
  # accidents, not reflection (Ruby binds a named subclass's constant before
  # `inherited` raises; `Marshal.load` and a hand-bound `Data#initialize` skip
  # the check), so the extension is the authority: every value it reads back
  # must be exactly its class and is checked again, TypeError or RangeError
  # otherwise.
  [SagaId, AttemptId, ActionKey, Action, Forward, Compensating, Settlement, Snapshot].each do |value|
    value.singleton_class.undef_method(:allocate)
    value.define_singleton_method(:inherited) { |_| raise TypeError, "#{value} cannot be subclassed" }
  end

  class Saga
    class << self
      # The core's `Saga::<STEPS>::new(id)`: a fresh workflow in scope zero.
      def new(steps, id) = with_scope(steps, id, 0)

      # The core's `Saga::<STEPS>::with_scope(id, scope)`: a fresh workflow
      # whose attempts carry `scope`, an Integer in 0..65_535.
      def with_scope(steps, id, scope) = native_with_scope(steps, id, scope)

      # The core's `Saga::<STEPS>::restored(snapshot, scope)`. RangeError
      # where the core answers None: a snapshot of another scope, or one no
      # saga of these steps can have reached.
      def restored(steps, snapshot, scope) = native_restored(steps, snapshot, scope)

      private :native_with_scope, :native_restored, :native_scope_of, :native_from_bytes,
              :native_saga_id, :native_step, :native_key, :native_action, :native_settlement,
              :native_snapshot
    end

    def id = snapshot.id
    def scope = snapshot.scope
    def issued = snapshot.issued
    def progress = snapshot.progress
    def settled = snapshot.settled

    def snapshot = Wire.snapshot(native_snapshot)

    def pending
      row = native_pending
      row && Wire.action(row)
    end

    def resolve(action, outcome) = native_resolve(action, outcome)
  end
end
